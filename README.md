# Prefabric

An open-source desktop IDE for **Unity and C#**, with a Rust core and a native
(Tauri) shell. Prefabric is a fork of [OpenIDEA](https://github.com/l0ng-ai/openidea)
and keeps its editor, code insight, debugger, Git, terminal and plugin system,
then focuses on game development with Unity and .NET.

> **Status: early.** No releases yet. Build it from source (below).

## What works today

- **C#**: completion, navigation and refactors through `csharp-ls`. Run configurations from
  `.sln`/`.slnx`/`.csproj` and `launchSettings.json`, `dotnet build` and `dotnet test`
  runs, and launch debugging through `netcoredbg`.
- **Unity projects**:
  - detects the project and its editor version;
  - a Unity view of the project tree that hides `Library/`, `Temp/` and `.meta` files;
  - moving or renaming an asset keeps its `.meta` file and GUID;
  - duplicating an asset gives the copy a fresh GUID and rewrites its references;
  - new `.meta` files are staged in Git with their assets.
- Everything inherited from OpenIDEA: many languages through LSP, debugging through DAP,
  Git, terminal, database and HTTP clients, and sandboxed WebAssembly plugins.

## Roadmap

The plan is in [`docs/proposals/rust-csharp-unity.md`](docs/proposals/rust-csharp-unity.md).
The main items:

1. **Unity external script editor**: a `dev.prefabric.unity` Unity package, so Prefabric can be
   picked in _Preferences › External Tools_ and generates the `.sln`/`.csproj` files.
2. **Asset-aware navigation**: index scenes, prefabs and assets, then find the GameObjects and
   prefabs that use a script, method or serialized field.
3. **Unity inspections**: event functions such as `Update` are no longer reported as unused,
   plus Unity analyzers and message completion.
4. **Unity debugger**: attach to the Unity Editor and players (Mono soft debugger).
5. **Editor link**: Play, Pause and Refresh from the IDE, the Unity console, and the Unity Test Runner.
6. A better C# experience: Roslyn language server, a solution view, NuGet and a test tree.

## Build from source

Requirements:

- [Rust](https://rustup.rs) (stable)
- Node.js 22 LTS (22.18 or newer)
- pnpm
- the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

```sh
pnpm install
pnpm tauri dev      # debug build with hot reload
pnpm tauri build    # release bundle in target/release/bundle/
```

The first build compiles about a thousand crates and can take a while; later builds are
incremental. Contributor notes are in [`CLAUDE.md`](CLAUDE.md) and
[`CONTRIBUTING.md`](CONTRIBUTING.md). Plugin docs: [`plugins/README.md`](plugins/README.md).

### Contributors

<a href="https://github.com/l0ng-ai/openidea/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=l0ng-ai/openidea&max=100" alt="OpenIDEA contributors">
</a>

## License

[Apache 2.0](LICENSE). Prefabric is based on OpenIDEA; see [`NOTICE`](NOTICE) for
attribution and [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) for bundled assets.

<sub>Not affiliated with, endorsed by or sponsored by JetBrains s.r.o. Rider, IntelliJ and
IntelliJ IDEA are trademarks of JetBrains s.r.o.</sub>
