# Prefabric

An open-source desktop IDE for **Unity and C#**, with a Rust core and a native
(Tauri 2) shell. Prefabric is a fork of [OpenIDEA](https://github.com/l0ng-ai/openidea)
that keeps its editor, code insight, debugger, Git, terminal, remote development and
plugin system, then focuses on game development with Unity and .NET.

> **Status: early preview.** No signed releases yet — build from source below or run the
> manual macOS build workflow in GitHub Actions.

## Features

### Unity

- **External script editor (`dev.prefabric.unity`)**: bundled and installed automatically
  for Unity 2022.3+. Select Prefabric in _Preferences › External Tools_ to open scripts
  and Console errors at line/column, and generate SDK-style `.csproj` / `.slnx` files with
  `Microsoft.Unity.Analyzers` (or use Unity's `UNITY_MSBUILD_ENABLED` pipeline). See
  [`unity/README.md`](unity/README.md).
- **Unity Editor Link**: Play, Pause, Step and Refresh controls in the title bar, a live
  **Unity Console** tool window, Editor compile diagnostics in Problems, **Show in Unity
  Editor** from code or YAML, and running/cancelling EditMode & PlayMode tests through the
  Unity Test Runner.
- **Unity debugger**: attach via Mono Soft Debugger to local Unity Editors, local and
  network development players (multicast discovery), and USB-connected Android (`adb`) and
  iOS (`iproxy`) devices, with support for pausepoints.
- **Asset index, Hierarchy & Code Vision**:
  - indexes `.unity` scenes, `.prefab` files (including variants and nested prefabs),
    animation clips and `.asset` files;
  - **Hierarchy view** for scenes and prefabs with components, prefab overrides, added and
    removed GameObjects, and read-only YAML inspection;
  - **Code Vision lenses & asset-aware navigation** for script usages, UnityEvent and
    animation event bindings, and serialized field values/overrides;
  - a **Unity** tab in Search Everywhere across GameObjects, prefab instances and assets.
- **Unity C# & asset editing**:
  - event function completion, documentation and gutter icons;
  - Unity-aware rename (offers matching `.cs` file rename and `[FormerlySerializedAs]` for
    serialized fields);
  - navigation in `.asmdef` / `.asmref` files and Unity property completion in `.uss`;
  - Unity project tree hiding `Library/`, `Temp/` and `.meta` clutter; moving, renaming or
    duplicating assets preserves or regenerates `.meta` GUIDs and rewrites references; new
    `.meta` files stage automatically in Git.

### C# & .NET

- **Roslyn Language Server** (`roslyn-language-server`, with `csharp-ls` fallback) for
  completion (including unimported namespaces), navigation, inlay hints, full-solution
  compiler diagnostics, refactors and `.editorconfig` formatting.
- **.NET tool window**: solution and project explorer, configuration / target-framework
  switcher, and NuGet package management.
- **Run, test & debug**: auto-detected configurations from `.sln` / `.slnx` / `.csproj` and
  `launchSettings.json`, `dotnet build` and `dotnet test` with test tree integration, and
  launch debugging through `netcoredbg`.

### Platform (from OpenIDEA)

- Multi-language LSP & tree-sitter support, DAP debugging, Git/VCS, integrated terminal,
  database and HTTP clients, remote development (SSH, WSL, Dev Containers), AI agents &
  MCP, and sandboxed WebAssembly plugins.
- Per-project configuration lives in `.prefabric/` (existing `.openidea/` folders continue
  to work automatically).

## Build from source

Requirements:

- [Rust](https://rustup.rs) (stable, 1.96+ / Edition 2024)
- Node.js 22 LTS (22.18 or newer)
- pnpm 12
- the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

```sh
pnpm install
pnpm tauri dev      # debug build with hot reload
```

To build a local unsigned release bundle (disabling updater artifact signing, which
otherwise requires `TAURI_SIGNING_PRIVATE_KEY`):

```sh
pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The first build compiles around a thousand crates and can take a while; later builds are
incremental.

### macOS CI builds

You can also trigger [`.github/workflows/build-macos.yml`](.github/workflows/build-macos.yml)
via **Actions › Build macOS › Run workflow** (`aarch64-apple-darwin`,
`x86_64-apple-darwin`, or `universal-apple-darwin`) to download an unsigned `.dmg` and
`.app.zip`. Before launching an unsigned build on macOS, clear the quarantine attribute:

```sh
xattr -cr /Applications/Prefabric.app
```

## Documentation & contributing

- Contributor guide: [`AGENTS.md`](AGENTS.md) (`CLAUDE.md`) and [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Unity package: [`unity/README.md`](unity/README.md)
- Plugins: [`plugins/README.md`](plugins/README.md)
- Architecture & design notes: [`docs/proposals/rust-csharp-unity.md`](docs/proposals/rust-csharp-unity.md)

### Contributors

<a href="https://github.com/l0ng-ai/openidea/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=l0ng-ai/openidea&max=100" alt="OpenIDEA contributors">
</a>

## License

[Apache 2.0](LICENSE). Prefabric is based on OpenIDEA; see [`NOTICE`](NOTICE) for
attribution and [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) for bundled assets.

<sub>Not affiliated with, endorsed by or sponsored by JetBrains s.r.o. Rider, IntelliJ and
IntelliJ IDEA are trademarks of JetBrains s.r.o.</sub>
