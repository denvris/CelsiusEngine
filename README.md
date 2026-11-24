# Celsius Engine

Celsius Engine is a modular PCVR engine prototype for 64-bit Windows. Rust and OpenXR handle the headset session, hand and body tracking, locomotion, and stereo rendering. Lua 5.4 defines gameplay, objects, and interactions. The included Baseplate demo lets you walk in place without joysticks and trigger a beacon by pinching your thumb and index finger.

The current build has been tested with simulated input and the Direct3D 11 renderer. Quest 3S is the first hardware target via Meta Horizon Link. Live headset tracking and frame timing are pending validation.

## Run the baseplate

Install stable Rust with the MSVC toolchain and Visual Studio Build Tools with **Desktop development with C++**. From the root of this repository, build a playable folder:

```powershell
.\examples\build-game-test.ps1 -Destination "$PWD\GameTest"
```

Open Meta Horizon Link in the headset, then double-click `GameTest\Jugar-Quest.cmd`. The launcher looks for the OpenXR loader next to the game or in a default SteamVR installation. You can also set `CELSIUS_OPENXR_LOADER` to an absolute path.

Use `GameTest\Jugar-Quest.cmd --probe` to check the active runtime. If the probe reports `HMD unavailable`, confirm that the headset has entered Link and that Meta Horizon Link is the active OpenXR runtime. The executable runs on the PC through Link. A standalone Quest build is outside this version.

### Run with Cargo

```powershell
cargo test --locked --all-targets
cargo run --locked --release --example baseplate
```

The example needs the Khronos OpenXR loader DLL. If the loader is outside the application's DLL search directories, pass its absolute path in `--loader` or set `CELSIUS_OPENXR_LOADER`. For example, a default SteamVR installation can provide the loader while Meta Horizon Link remains the active runtime:

```powershell
$env:CELSIUS_OPENXR_LOADER = 'C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\openxr_loader.dll'
cargo run --locked --release --example baseplate -- --probe
cargo run --locked --release --example baseplate
```

For Quest 3S, enable hand tracking and the developer runtime features available in the Link app. The probe prints the headset system name and whether the runtime supplies articulated hands and body tracking. The demo can open with head tracking alone when hands are unavailable.

Walk or jog in place with a small, regular vertical bounce. The engine filters headset height and confirms a cadence from successive peaks and troughs. An isolated crouch does not start movement. It adjusts speed to the pace, follows the headset's horizontal heading, and eases to a stop when the rhythm ends. Thumb-to-index pinches leave colored markers and pulse the beacon. Look down to see the avatar. The headset supplies the head pose and valid hand joints, including fingers. The avatar uses torso and arm joints from `XR_FB_body_tracking` and lower-body joints from `XR_META_body_tracking_full_body` when the runtime supplies them. The runtime may synthesize leg poses. When those joints are unavailable, the engine estimates the pose from the head, hands, and locomotion. `--probe` reports runtime capability for both extensions.

### Desktop gameplay mirror

OpenXR projects open a Windows window named **Celsius Engine - VR Gameplay** by default. In OBS, add a Window Capture source and select that window. It shows the rendered left eye at the largest size that fits the desktop work area, up to the eye's native render resolution, and preserves the eye's aspect ratio. The desktop uses an asynchronous GPU copy of the existing frame and can skip presentation when its swapchain is busy. Closing the desktop window leaves VR running. Disable it with `--no-mirror` or set `SessionOptions::desktop_mirror` to false when calling `openxr::run_session`.

### Preview on Windows without a headset

```powershell
cargo run --locked --example baseplate -- --preview target/baseplate.png
```

This runs a simulated head movement and pinch through the real Lua and D3D11 paths. The resulting third-person PNG shows the demo pose. It uses the Windows software renderer, so its rendering time does not measure PCVR performance.

## Gameplay in Lua

Select a Lua module at launch with `--game path/to/game.lua`, or use [examples/baseplate.lua](examples/baseplate.lua) as a starting point. The Lua module returns a table. Its `objects` array can add, move, recolor, and remove renderable boxes each frame. Use `position`, `size` and `color` as three-element arrays in metres and linear RGB, with optional `yaw` in radians and `material` as a numeric shader identifier. The default surface shader treats 1 as a grid surface and 2 as emissive.

| Optional callback | Invoked when |
| --- | --- |
| `on_hand_pose(hand, joints)` | Each frame for both hands. Joints are keyed by name and contain `position` and `orientation` in tracking space, with the orientation quaternion in x, y, z, w order. An empty table clears the hand. |
| `process_hand_gesture(gesture, hand)` | When a thumb-to-index pinch begins. The current hand pose has already been delivered. Custom gestures can also be detected in Lua from joint positions. |
| `on_locomotion_step()` | When the headset completes a bounce in the established walk or jog cadence. |
| `on_frame(dt, offset)` | After the tracking events. Add the horizontal world offset to tracking positions when placing game objects. |

Set `step_speed` in m/s to tune the initial pulse of motion, or use 0 to disable movement. `movement_limit` sets the world-offset limits around the origin. `cloud_density` controls the sky volume and uses 0 to skip it.

## Renderer and custom shaders

The raster renderer draws the Lua scene and avatar into separate per-eye sRGB swapchains, with depth testing, a shared sun shadow map, linear lighting, filmic tone mapping, and an optional volumetric cloud layer. The demos default to the runtime's full recommended eye resolution. Use `--scale 0.4..1.25` to trade resolution for performance or request supersampling. The request is capped at the runtime's advertised maximum, and the actual dimensions are printed at startup.

Set `volumetric_density` in Lua to enable low-lying fog. The fog samples the sun shadow map along each view ray, so scene geometry can block the light inside the volume. Reflection Lab enables it by default. Override it with `--fog 0..0.5` to compare. Sun shadows in fog use eight samples per ray and the sky runs only where scene geometry leaves the depth buffer uncovered.

OpenXR swapchain images use an explicit D3D11 render-target view in the selected sRGB format. This supports runtimes that return typeless swapchain textures. The desktop PNG preview exercises the same typed-view creation against a typeless texture.

- `--hdr path/to/sky.hdr` loads a linear equirectangular Radiance HDR image for the sky and directional environmental color. The current ambient term samples the panorama directly.
- `--shader path/to/surface.hlsl` replaces the default surface function. Start from [default_surface.hlsl](src/graphics/default_surface.hlsl). Supply `float3 shade_surface(Surface s)` and return a linear color. `s` exposes world position, normal, vertex color, and the Lua material identifier. Shader code also has access to the current eye's camera and view/projection matrices, `environment(ray)`, and `sunlight_visibility(world, normal)`. The engine applies exposure-independent tone mapping and handles stereo submission.

The currently supported geometry is boxes, segments and spheres. General mesh import, production PBR, and native Quest packaging remain future work.

## Reflection Lab

`Probar-Reflejos.cmd` opens a second PCVR test alongside the original Baseplate launcher. It shows first-person hands in cyan and orange. When optical tracking is available, the model follows the tracked fingers. If a hand loses optical tracking while a controller is tracked, the model follows the grip with a fixed finger pose. Controller poses do not generate optical pinch events. Use the same march-in-place movement as Baseplate to approach the spheres. Six spheres show chrome, brushed metal, solid glass, glazed ceramic, varnished wood, and concrete. The high-contrast panel has five progressively finer line groups, and a second panel behind the starting viewpoint makes scene reflections easy to distinguish from sky reflections. The glass sphere uses a refractive index of 1.50, separate entry and exit refraction, Fresnel reflectance, and thickness-based absorption.

For the portable build, extract the complete Reflection Lab ZIP to its own folder and run the launcher beside its bundled `baseplate.exe`. `Unknown option: --lab` means the executable is from the earlier Baseplate-only build. The launcher writes the most recent startup output to `%TEMP%\CelsiusEngine-ReflectionLab.log`. Use `Probar-Reflejos.cmd --probe` to check headset detection with no renderer startup, or `Probar-Reflejos.cmd --preview reflection-preview.png` to test desktop rendering independently of Link. If the runtime reports broker IPC errors or an unavailable headset, open Meta Horizon Link on the PC and first confirm that you can enter the Link environment inside the headset.

```powershell
# Optional 4K materials and photographed HDR sky. Run once from the repository root.
.\examples\download-reflection-assets.ps1

# Compare the same scene with scene ray queries on and off.
cargo run --locked --release --example baseplate -- --lab --preview target/reflections-on.png
cargo run --locked --release --example baseplate -- --lab --raytracing off --preview target/reflections-off.png
cargo run --locked --release --example baseplate -- --walk-preview target/reflections-walk.png

# In Quest Link, start with the lab default. Lower the per-eye scale to compare performance.
.\GameTest\Probar-Reflejos.cmd
.\GameTest\Probar-Reflejos.cmd --scale 1.15
.\GameTest\Probar-Reflejos.cmd --scale 0.65
.\GameTest\Probar-Reflejos.cmd --scale 0.65 --raytracing off
.\GameTest\Probar-Reflejos.cmd --fog 0
```

The downloader fetches the color and roughness maps for [Concrete Floor](https://polyhaven.com/a/concrete_floor) and [Wood Table 001](https://polyhaven.com/a/wood_table_001), plus the photographed [Kloppenheim 05 Pure Sky](https://polyhaven.com/a/kloppenheim_05_puresky), all from [Poly Haven under CC0](https://polyhaven.com/license). The lab uses procedural materials and sky if those files are missing. Albedo uses sRGB sampling, roughness uses a linear single-channel texture, and both use mipmaps and anisotropic filtering. The HDRI supplies the sky and reflected radiance. Volumetric clouds are sampled in the reflected and transmitted sky as well as in the main sky view.

### Enable ray tracing in a game

Set `ray_tracing = true` in the Lua game table and use a surface shader that calls `trace_radiance(origin, direction)` with a normalized direction. The default raster shader does not call it. `--raytracing on|off` overrides the demo's Lua setting. With the flag off, the helper samples the environment and skips all scene intersections. See [reflection_lab.lua](examples/reflection_lab.lua) and [reflection_lab.hlsl](examples/reflection_lab.hlsl).

Primary visibility is rasterized for both modes. The D3D11 pixel shader recalculates reflections and refraction per eye each rendered frame. The engine uploads up to 48 current sphere and oriented-box transforms within the ray scene, with a 35 m ray limit. Secondary hits use approximate shading. The lab opts into reflecting the avatar's palms and legs with `trace_avatar = true` and animates a green target behind the starting view to make reflection updates visible in chrome. Look down to see the legs move with the stride. The lab casts one scene ray for chrome, ceramic, and varnished wood, and up to two for glass or brushed metal. The renderer uses Shader Model 5 and analytic intersections. Headset frame rate and behavior across GPU vendors have yet to be measured.

The original smoke examples remain available with `cargo run --locked --example progression` and `cargo run --locked --example vr_input`.

## Source layout

| Path | Responsibility |
| --- | --- |
| [src/vr](src/vr) | OpenXR frame loop, tracking, gesture detection and locomotion |
| [src/scripting](src/scripting) | Lua scene and object bindings |
| [src/graphics](src/graphics) | D3D11 rendering, environment lighting and HLSL shaders |
| [examples](examples) | Baseplate demo, game test launcher and examples |

Update this README in the same change whenever build steps, public APIs, supported features, or demo controls change.

## References

- [Khronos OpenXR specification](https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html)
- [Meta Horizon Link development setup](https://developers.meta.com/vr/documentation/unreal/unreal-link/)
