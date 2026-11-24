use std::{error::Error, ffi::c_void, path::Path, ptr, slice, sync::OnceLock};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use windows::{
    Win32::{
        Foundation::{HMODULE, LUID},
        Graphics::{
            Direct3D::{Fxc::D3DCompile, *},
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
    core::{Interface, PCSTR, s},
};

use super::{BoxMesh, Camera, Scene, SphereMesh};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const DEFAULT_SURFACE: &str = include_str!("default_surface.hlsl");

pub fn create_device(adapter: Option<(LUID, D3D_FEATURE_LEVEL)>) -> Result<ID3D11Device> {
    let (adapter, driver, levels) = if let Some((luid, minimum)) = adapter {
        let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1()? };
        let adapter: IDXGIAdapter = unsafe { factory.EnumAdapterByLuid(luid)? };
        let levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]
            .into_iter()
            .filter(|level| level.0 >= minimum.0)
            .collect::<Vec<_>>();
        (Some(adapter), D3D_DRIVER_TYPE_UNKNOWN, levels)
    } else {
        (None, D3D_DRIVER_TYPE_WARP, vec![D3D_FEATURE_LEVEL_11_0])
    };
    if levels.is_empty() {
        return Err("The OpenXR runtime requires a D3D feature level above 11.1".into());
    }
    let mut device = None;
    unsafe {
        D3D11CreateDevice(
            adapter.as_ref(),
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
    }
    Ok(device.expect("D3D11CreateDevice output"))
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
    material: f32,
    sphere: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Eye {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    light_view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    settings: [f32; 4],
    material_maps: [f32; 4],
}

const MAX_RAY_OBJECTS: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RayObject {
    center_radius: [f32; 4],
    extent_material: [f32; 4],
    color: [f32; 4],
}

const MATERIAL_MAPS: [&str; 4] = [
    "concrete_floor_diff_4k.jpg",
    "concrete_floor_rough_4k.jpg",
    "wood_table_001_diff_4k.jpg",
    "wood_table_001_rough_4k.jpg",
];

pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    sky_vs: ID3D11VertexShader,
    sky_ps: ID3D11PixelShader,
    mesh_vs: ID3D11VertexShader,
    mesh_ps: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    rasterizer: ID3D11RasterizerState,
    sampler: ID3D11SamplerState,
    shadow: ShadowMap,
    sky: Option<ID3D11ShaderResourceView>,
    eye: ID3D11Buffer,
    ray_buffer: ID3D11Buffer,
    ray_count: usize,
    ray_tracing: bool,
    maps: [Option<ID3D11ShaderResourceView>; 4],
    material_sampler: ID3D11SamplerState,
    vertex_buffer: ID3D11Buffer,
    vertex_capacity: usize,
    vertices: Vec<Vertex>,
}

struct ShadowMap {
    depth: ID3D11DepthStencilView,
    view: ID3D11ShaderResourceView,
    sampler: ID3D11SamplerState,
    projection: [[f32; 4]; 4],
}

const SHADOW_SIZE: u32 = 2048;

impl ShadowMap {
    fn new(device: &ID3D11Device) -> Result<Self> {
        let map = texture(
            device,
            texture_desc(
                SHADOW_SIZE,
                SHADOW_SIZE,
                DXGI_FORMAT_R32_TYPELESS,
                D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
            ),
            None,
        )?;
        let depth_desc = D3D11_DEPTH_STENCIL_VIEW_DESC {
            Format: DXGI_FORMAT_D32_FLOAT,
            ViewDimension: D3D11_DSV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_DSV { MipSlice: 0 },
            },
            Flags: 0,
        };
        let view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R32_FLOAT,
            ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                },
            },
        };
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_BORDER,
            AddressV: D3D11_TEXTURE_ADDRESS_BORDER,
            AddressW: D3D11_TEXTURE_ADDRESS_BORDER,
            BorderColor: [1.0; 4],
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_LESS_EQUAL,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let (mut depth, mut view, mut sampler) = (None, None, None);
        unsafe {
            device.CreateDepthStencilView(&map, Some(&depth_desc), Some(&mut depth))?;
            device.CreateShaderResourceView(&map, Some(&view_desc), Some(&mut view))?;
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
        }
        let sun = Vec3::new(-0.3578, 0.7155, -0.6000).normalize();
        let projection = Mat4::orthographic_rh(-15.0, 15.0, -15.0, 15.0, 1.0, 65.0)
            * Mat4::look_at_rh(sun * 28.0, Vec3::ZERO, Vec3::Y);
        Ok(Self {
            depth: depth.unwrap(),
            view: view.unwrap(),
            sampler: sampler.unwrap(),
            projection: projection.to_cols_array_2d(),
        })
    }
}

pub struct RenderTarget {
    colors: Vec<Option<ID3D11RenderTargetView>>,
    depth: ID3D11DepthStencilView,
    viewport: D3D11_VIEWPORT,
}

impl Renderer {
    pub fn new(device: ID3D11Device, surface: &str, hdri: Option<&Path>) -> Result<Self> {
        let source = format!(
            "{}\n{}\n{}",
            include_str!("shaders.hlsl"),
            include_str!("ray_tracing.hlsl"),
            surface
        );
        let sky_vs = compile(&source, s!("sky_vs"), s!("vs_5_0"))?;
        let sky_ps = compile(&source, s!("sky_ps"), s!("ps_5_0"))?;
        let mesh_vs = compile(&source, s!("mesh_vs"), s!("vs_5_0"))?;
        let mesh_ps = compile(&source, s!("mesh_ps"), s!("ps_5_0"))?;
        let elements = [
            (s!("POSITION"), 0, DXGI_FORMAT_R32G32B32_FLOAT, 0),
            (s!("NORMAL"), 0, DXGI_FORMAT_R32G32B32_FLOAT, 12),
            (s!("COLOR"), 0, DXGI_FORMAT_R32G32B32_FLOAT, 24),
            (s!("TEXCOORD"), 0, DXGI_FORMAT_R32_FLOAT, 36),
            (s!("TEXCOORD"), 1, DXGI_FORMAT_R32G32B32A32_FLOAT, 40),
        ]
        .map(|(name, index, format, offset)| D3D11_INPUT_ELEMENT_DESC {
            SemanticName: name,
            SemanticIndex: index,
            Format: format,
            AlignedByteOffset: offset,
            ..Default::default()
        });
        let mut layout = None;
        let rasterizer = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        };
        let sampler = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let (mut rasterizer_state, mut sampler_state, mut material_sampler) = (None, None, None);
        let context = unsafe { device.GetImmediateContext()? };
        unsafe {
            device.CreateInputLayout(&elements, &mesh_vs, Some(&mut layout))?;
            device.CreateRasterizerState(&rasterizer, Some(&mut rasterizer_state))?;
            device.CreateSamplerState(&sampler, Some(&mut sampler_state))?;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_ANISOTROPIC,
                    AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
                    MaxAnisotropy: 8,
                    ..sampler
                },
                Some(&mut material_sampler),
            )?;
        }
        let vertex_capacity = 4096;
        Ok(Self {
            context,
            sky_vs: vertex_shader(&device, &sky_vs)?,
            sky_ps: pixel_shader(&device, &sky_ps)?,
            mesh_vs: vertex_shader(&device, &mesh_vs)?,
            mesh_ps: pixel_shader(&device, &mesh_ps)?,
            layout: layout.unwrap(),
            rasterizer: rasterizer_state.unwrap(),
            sampler: sampler_state.unwrap(),
            shadow: ShadowMap::new(&device)?,
            sky: hdri.map(|path| load_hdr(&device, path)).transpose()?,
            eye: dynamic_buffer(&device, size_of::<Eye>(), D3D11_BIND_CONSTANT_BUFFER)?,
            ray_buffer: dynamic_buffer(
                &device,
                size_of::<[RayObject; MAX_RAY_OBJECTS]>(),
                D3D11_BIND_CONSTANT_BUFFER,
            )?,
            ray_count: 0,
            ray_tracing: false,
            maps: Default::default(),
            material_sampler: material_sampler.unwrap(),
            vertex_buffer: dynamic_buffer(
                &device,
                vertex_capacity * size_of::<Vertex>(),
                D3D11_BIND_VERTEX_BUFFER,
            )?,
            vertex_capacity,
            vertices: Vec::with_capacity(vertex_capacity),
            device,
        })
    }

    pub fn load_material_maps(&mut self, directory: &Path) -> Result<usize> {
        let mut loaded = 0;
        for (i, name) in MATERIAL_MAPS.iter().enumerate() {
            let path = directory.join(name);
            if path.try_exists()? {
                self.maps[i] = Some(load_material_map(
                    &self.device,
                    &self.context,
                    &path,
                    i % 2 == 0,
                )?);
                loaded += 1;
            }
        }
        Ok(loaded)
    }

    pub fn target(
        &self,
        images: &[ID3D11Texture2D],
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<RenderTarget> {
        let mut colors = Vec::with_capacity(images.len());
        let mut depth = None;
        let depth_texture = texture(
            &self.device,
            texture_desc(
                width,
                height,
                DXGI_FORMAT_D32_FLOAT,
                D3D11_BIND_DEPTH_STENCIL,
            ),
            None,
        )
        .map_err(|error| format!("Creating D3D11 depth texture ({width}x{height}): {error}"))?;
        // OpenXR can expose typeless textures. Bind the concrete swapchain format.
        let color_view = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_RTV { MipSlice: 0 },
            },
        };
        unsafe {
            for (index, image) in images.iter().enumerate() {
                let mut rtv = None;
                self.device
                    .CreateRenderTargetView(image, Some(&color_view), Some(&mut rtv))
                    .map_err(|error| {
                        let mut desc = D3D11_TEXTURE2D_DESC::default();
                        image.GetDesc(&mut desc);
                        format!("Creating D3D11 color view for image {index} ({width}x{height}, resource format {}, view format {}): {error}", desc.Format.0, format.0)
                    })?;
                colors.push(rtv);
            }
            self.device
                .CreateDepthStencilView(&depth_texture, None, Some(&mut depth))
                .map_err(|error| {
                    format!("Creating D3D11 depth view ({width}x{height}): {error}")
                })?;
        }
        Ok(RenderTarget {
            colors,
            depth: depth.unwrap(),
            viewport: D3D11_VIEWPORT {
                Width: width as f32,
                Height: height as f32,
                MaxDepth: 1.0,
                ..Default::default()
            },
        })
    }

    pub fn upload(&mut self, scene: &Scene) -> Result<()> {
        self.vertices.clear();
        for mesh in &scene.boxes {
            append_box(&mut self.vertices, mesh);
        }
        for mesh in &scene.spheres {
            append_sphere(&mut self.vertices, mesh);
        }
        self.ray_tracing = scene.ray_tracing;
        if self.ray_tracing {
            self.upload_ray_objects(scene)?;
        } else {
            self.ray_count = 0;
        }
        if self.vertices.len() > self.vertex_capacity {
            let capacity = self.vertices.len().next_power_of_two();
            self.vertex_buffer = dynamic_buffer(
                &self.device,
                capacity * size_of::<Vertex>(),
                D3D11_BIND_VERTEX_BUFFER,
            )?;
            self.vertex_capacity = capacity;
        }
        self.write_buffer(&self.vertex_buffer, bytemuck::cast_slice(&self.vertices))?;
        self.write_buffer(
            &self.eye,
            bytemuck::bytes_of(&Eye {
                view_projection: self.shadow.projection,
                ..Eye::zeroed()
            }),
        )?;
        let ctx = &self.context;
        unsafe {
            ctx.PSSetShaderResources(1, Some(&[None]));
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: SHADOW_SIZE as f32,
                Height: SHADOW_SIZE as f32,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            ctx.RSSetState(&self.rasterizer);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.ClearDepthStencilView(&self.shadow.depth, D3D11_CLEAR_DEPTH.0, 1.0, 0);
            ctx.OMSetRenderTargets(None, &self.shadow.depth);
            ctx.PSSetShader(None::<&ID3D11PixelShader>, None);
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.eye.clone())]));
            self.draw_mesh();
            ctx.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
        }
        Ok(())
    }

    pub fn draw(
        &self,
        target: &RenderTarget,
        index: usize,
        camera: Camera,
        cloud_density: f32,
    ) -> Result<()> {
        let view_projection = camera.view_projection();
        self.write_buffer(
            &self.eye,
            bytemuck::bytes_of(&Eye {
                view_projection: view_projection.to_cols_array_2d(),
                inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
                light_view_projection: self.shadow.projection,
                camera: camera.position.extend(1.0).to_array(),
                settings: [
                    f32::from(u8::from(self.sky.is_some())),
                    cloud_density,
                    u8::from(self.ray_tracing) as f32,
                    self.ray_count as f32,
                ],
                material_maps: self
                    .maps
                    .each_ref()
                    .map(|map| u8::from(map.is_some()) as f32),
            }),
        )?;
        let color = target
            .colors
            .get(index)
            .ok_or("Invalid swapchain image index")?;
        let targets = slice::from_ref(color);
        let ctx = &self.context;
        unsafe {
            ctx.RSSetViewports(Some(slice::from_ref(&target.viewport)));
            ctx.RSSetState(&self.rasterizer);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.OMSetRenderTargets(Some(targets), None::<&ID3D11DepthStencilView>);
            ctx.IASetInputLayout(None::<&ID3D11InputLayout>);
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.eye.clone())]));
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.eye.clone())]));
            ctx.PSSetShaderResources(0, Some(slice::from_ref(&self.sky)));
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            ctx.VSSetShader(&self.sky_vs, None);
            ctx.PSSetShader(&self.sky_ps, None);
            ctx.Draw(3, 0);

            ctx.ClearDepthStencilView(&target.depth, D3D11_CLEAR_DEPTH.0, 1.0, 0);
            ctx.OMSetRenderTargets(Some(targets), &target.depth);
            ctx.PSSetShaderResources(1, Some(&[Some(self.shadow.view.clone())]));
            ctx.PSSetSamplers(1, Some(&[Some(self.shadow.sampler.clone())]));
            if self.ray_tracing {
                ctx.PSSetConstantBuffers(1, Some(&[Some(self.ray_buffer.clone())]));
            }
            ctx.PSSetShaderResources(2, Some(&self.maps));
            ctx.PSSetSamplers(2, Some(&[Some(self.material_sampler.clone())]));
            ctx.PSSetShader(&self.mesh_ps, None);
            self.draw_mesh();
            ctx.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            // The XR compositor consumes these images on a separate device/context.
            ctx.Flush();
        }
        Ok(())
    }

    fn upload_ray_objects(&mut self, scene: &Scene) -> Result<()> {
        let spheres = scene.spheres.iter().map(|mesh| RayObject {
            center_radius: mesh.center.extend(mesh.radius).to_array(),
            extent_material: Vec3::ZERO.extend(mesh.material).to_array(),
            color: mesh.color.extend(1.0).to_array(),
        });
        let boxes = scene
            .boxes
            .iter()
            .take(scene.traced_boxes)
            .filter(|mesh| mesh.orientation.abs_diff_eq(glam::Quat::IDENTITY, 0.0001))
            .map(|mesh| RayObject {
                center_radius: mesh.center.extend(0.0).to_array(),
                extent_material: (mesh.size * 0.5).extend(mesh.material).to_array(),
                color: mesh.color.extend(1.0).to_array(),
            });
        let mut objects = [RayObject::zeroed(); MAX_RAY_OBJECTS];
        self.ray_count = 0;
        for (slot, object) in objects.iter_mut().zip(spheres.chain(boxes)) {
            *slot = object;
            self.ray_count += 1;
        }
        self.write_buffer(&self.ray_buffer, bytemuck::cast_slice(&objects))
    }

    fn draw_mesh(&self) {
        let ctx = &self.context;
        unsafe {
            ctx.IASetInputLayout(&self.layout);
            ctx.IASetVertexBuffers(
                0,
                1,
                Some(&Some(self.vertex_buffer.clone())),
                Some(&(size_of::<Vertex>() as u32)),
                Some(&0),
            );
            ctx.VSSetShader(&self.mesh_vs, None);
            ctx.Draw(self.vertices.len() as u32, 0);
        }
    }

    pub fn capture_png(
        &self,
        width: u32,
        height: u32,
        camera: Camera,
        clouds: f32,
        path: &Path,
    ) -> Result<()> {
        let desc = texture_desc(
            width,
            height,
            DXGI_FORMAT_R8G8B8A8_TYPELESS,
            D3D11_BIND_RENDER_TARGET,
        );
        let color = texture(&self.device, desc, None)?;
        let target = self.target(
            slice::from_ref(&color),
            width,
            height,
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        )?;
        self.draw(&target, 0, camera, clouds)?;

        let staging = texture(
            &self.device,
            D3D11_TEXTURE2D_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                Usage: D3D11_USAGE_STAGING,
                ..desc
            },
            None,
        )?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context.CopyResource(&staging, &color);
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            for row in 0..height as usize {
                let ptr = mapped
                    .pData
                    .cast::<u8>()
                    .add(row * mapped.RowPitch as usize);
                pixels.extend_from_slice(slice::from_raw_parts(ptr, width as usize * 4));
            }
            self.context.Unmap(&staging, 0);
        }
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
        Ok(())
    }

    fn write_buffer(&self, buffer: &ID3D11Buffer, bytes: &[u8]) -> Result<()> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.pData.cast(), bytes.len());
            self.context.Unmap(buffer, 0);
        }
        Ok(())
    }
}

fn append_box(vertices: &mut Vec<Vertex>, mesh: &BoxMesh) {
    let half_size = mesh.size * 0.5;
    for (axis, u, v) in [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::Z, Vec3::X, Vec3::Y),
    ] {
        for side in [-1.0, 1.0] {
            let normal = axis * side;
            for (a, b) in [
                (-1.0, -1.0),
                (1.0, -1.0),
                (1.0, 1.0),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
            ] {
                let local = (normal + u * a + v * b) * half_size;
                vertices.push(Vertex {
                    position: (mesh.center + mesh.orientation * local).to_array(),
                    normal: (mesh.orientation * normal).to_array(),
                    color: mesh.color.to_array(),
                    material: mesh.material,
                    sphere: [0.0; 4],
                });
            }
        }
    }
}

fn append_sphere(vertices: &mut Vec<Vertex>, mesh: &SphereMesh) {
    static UNIT_SPHERE: OnceLock<Vec<Vec3>> = OnceLock::new();
    let normals = UNIT_SPHERE.get_or_init(|| {
        let (rings, sectors) = (24, 48);
        let point = |r, s| {
            let (y_sin, y_cos) = (std::f32::consts::PI * r as f32 / rings as f32).sin_cos();
            let (sin, cos) = (std::f32::consts::TAU * s as f32 / sectors as f32).sin_cos();
            Vec3::new(y_sin * sin, y_cos, -y_sin * cos)
        };
        let mut unit = Vec::new();
        for r in 0..rings {
            for s in 0..sectors {
                let (a, b, c, d) = (
                    point(r, s),
                    point(r + 1, s),
                    point(r + 1, s + 1),
                    point(r, s + 1),
                );
                if r + 1 < rings {
                    unit.extend([a, b, c]);
                }
                if r > 0 {
                    unit.extend([a, c, d]);
                }
            }
        }
        unit
    });
    let sphere = mesh.center.extend(mesh.radius).to_array();
    let color = mesh.color.to_array();
    vertices.extend(normals.iter().map(|normal| Vertex {
        position: (mesh.center + mesh.radius * normal).to_array(),
        normal: normal.to_array(),
        color,
        material: mesh.material,
        sphere,
    }));
}

fn dynamic_buffer(
    device: &ID3D11Device,
    bytes: usize,
    bind: D3D11_BIND_FLAG,
) -> Result<ID3D11Buffer> {
    let mut buffer = None;
    unsafe {
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: bytes.try_into()?,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: bind.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut buffer),
        )?;
    }
    Ok(buffer.unwrap())
}

fn texture_desc(
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    bind: D3D11_BIND_FLAG,
) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BindFlags: bind.0 as u32,
        ..Default::default()
    }
}

fn texture(
    device: &ID3D11Device,
    desc: D3D11_TEXTURE2D_DESC,
    data: Option<&D3D11_SUBRESOURCE_DATA>,
) -> Result<ID3D11Texture2D> {
    let mut output = None;
    unsafe {
        device.CreateTexture2D(&desc, data.map(ptr::from_ref), Some(&mut output))?;
    }
    Ok(output.unwrap())
}

fn load_hdr(device: &ID3D11Device, path: &Path) -> Result<ID3D11ShaderResourceView> {
    let pixels = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?
        .into_rgba32f();
    let desc = texture_desc(
        pixels.width(),
        pixels.height(),
        DXGI_FORMAT_R32G32B32A32_FLOAT,
        D3D11_BIND_SHADER_RESOURCE,
    );
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_raw().as_ptr().cast(),
        SysMemPitch: pixels.width() * 16,
        ..Default::default()
    };
    let texture = texture(device, desc, Some(&data))?;
    let mut output = None;
    unsafe {
        device.CreateShaderResourceView(&texture, None, Some(&mut output))?;
    }
    Ok(output.unwrap())
}

fn load_material_map(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    path: &Path,
    albedo: bool,
) -> Result<ID3D11ShaderResourceView> {
    let image = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?;
    let (format, bytes_per_pixel, pixels) = if albedo {
        (
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            4,
            image.to_rgba8().into_raw(),
        )
    } else {
        (DXGI_FORMAT_R8_UNORM, 1, image.to_luma8().into_raw())
    };
    let desc = D3D11_TEXTURE2D_DESC {
        MipLevels: 0,
        MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        ..texture_desc(
            image.width(),
            image.height(),
            format,
            D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
        )
    };
    let texture = texture(device, desc, None)?;
    let mut view = None;
    unsafe {
        device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
        context.UpdateSubresource(
            &texture,
            0,
            None,
            pixels.as_ptr().cast(),
            image.width() * bytes_per_pixel,
            0,
        );
    }
    let view = view.unwrap();
    unsafe {
        context.GenerateMips(&view);
    }
    Ok(view)
}

fn compile(source: &str, entry: PCSTR, model: PCSTR) -> Result<Vec<u8>> {
    let (mut shader, mut errors) = (None, None);
    unsafe {
        if let Err(error) = D3DCompile(
            source.as_ptr().cast::<c_void>(),
            source.len(),
            s!("Celsius.hlsl"),
            None,
            None::<&ID3DInclude>,
            entry,
            model,
            0,
            0,
            &mut shader,
            Some(&mut errors),
        ) {
            if let Some(errors) = errors {
                let bytes = slice::from_raw_parts(
                    errors.GetBufferPointer().cast::<u8>(),
                    errors.GetBufferSize(),
                );
                return Err(String::from_utf8_lossy(bytes)
                    .trim_end_matches('\0')
                    .to_owned()
                    .into());
            }
            return Err(error.into());
        }
        let shader = shader.unwrap();
        Ok(slice::from_raw_parts(
            shader.GetBufferPointer().cast::<u8>(),
            shader.GetBufferSize(),
        )
        .to_vec())
    }
}

fn vertex_shader(device: &ID3D11Device, bytes: &[u8]) -> Result<ID3D11VertexShader> {
    let mut shader = None;
    unsafe {
        device.CreateVertexShader(bytes, None::<&ID3D11ClassLinkage>, Some(&mut shader))?;
    }
    Ok(shader.unwrap())
}

fn pixel_shader(device: &ID3D11Device, bytes: &[u8]) -> Result<ID3D11PixelShader> {
    let mut shader = None;
    unsafe {
        device.CreatePixelShader(bytes, None::<&ID3D11ClassLinkage>, Some(&mut shader))?;
    }
    Ok(shader.unwrap())
}

/// D3D11 swapchain images are borrowed from OpenXR and valid until the swapchain is destroyed.
///
/// # Safety
/// The pointer must reference a live ID3D11Texture2D owned by the runtime.
pub unsafe fn borrow_image(raw: *mut c_void) -> Option<ID3D11Texture2D> {
    unsafe { ID3D11Texture2D::from_raw_borrowed(&raw).cloned() }
}
