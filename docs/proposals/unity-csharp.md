# Proposal: Unity and C# support that beats Rider

Status: plan v2.1, research and planning only · 2026-11-06

For Unity and C#, this document replaces sections 4–5 of
[`rust-csharp-unity.md`](rust-csharp-unity.md).

**Changes since v1:**

- Recorded the decisions in §1.
- Moved the benchmark milestone (M0) to the end, as M5.
- Moved the foundation work from M0 into the milestones that first need it.
- Added a task breakdown (§6).
- Added a review of the plan (§8). Its findings are already applied in this version.

---

## Progress (2026-11-07)

| Task | State          | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ---- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| N2   | done           | Product name Prefabric, bundle id `dev.prefabric.ide`, binary `prefabric`. Upstream attribution in icon comments stays "OpenIDEA".                                                                                                                                                                                                                                                                                                                                                                   |
| F2   | done           | `fixtures/csharp-app`: `.sln` and `.slnx`, library, console app, xUnit (8 tests, one opt-in failure with `OPENIDEA_FAIL=1`).                                                                                                                                                                                                                                                                                                                                                                         |
| F3   | done, narrowed | A `roslyn` server module in `ide-lsp` (the `angular` module's pattern) instead of a general hook: sends `solution/open` / `project/open` after `initialized`, counts the server as loading until `workspace/projectInitializationComplete`, answers `workspace/_roslyn_projectNeedsRestore`. Moving `jdtls`/`gopls` onto a general hook is left out: no second user yet.                                                                                                                             |
| F4   | done           | NuGet source with a required SHA-512 from the catalog; checked against the real `roslyn-language-server.osx-arm64` 5.11.0 package.                                                                                                                                                                                                                                                                                                                                                                   |
| F5   | done           | Private .NET 10 SDK, from Microsoft's release metadata with its SHA-512, installed on first .NET tool install when the system has no .NET 10 SDK. Not yet run against the real 200 MB download.                                                                                                                                                                                                                                                                                                      |
| F6   | done           | Build, run, debug and language servers were already off in Restricted Mode; a test now proves Roslyn is never contacted there. Found and fixed: `dotnet tool install` ran in the IDE's inherited working directory, whose `nuget.config` could redirect the package feed. It now runs in a private empty directory.                                                                                                                                                                                  |
| F1   | done           | `fixtures/unity-sample`, made with Unity 6000.6.8f1 (the installed version; not yet opened in 2022.3 or 6.3). Assets and SDK-style project files come from `Assets/Editor/FixtureBuilder.cs`; EditMode tests pass in batch mode and the projects build with `dotnet build`.                                                                                                                                                                                                                          |
| R1   | done           | Tool `roslyn`, `roslyn-language-server.{rid}` 5.13.0-1.26514.10 from NuGet, SHA-512 checked; wrapped with `DOTNET_ROOT` pointing at the .NET it runs on. Installed and ran `--version` on macOS (arm64) only.                                                                                                                                                                                                                                                                                        |
| R2   | done           | Roslyn first, csharp-ls with `fallback = true`. The choice is the existing per-server on/off switch: turning Roslyn off falls back to csharp-ls. Not yet checked in the app.                                                                                                                                                                                                                                                                                                                         |
| R3   | done           | Via F3; the server shows as busy until its projects load. The status-bar look is not yet checked in the app.                                                                                                                                                                                                                                                                                                                                                                                         |
| R4   | done           | `csharp.toml` answers Roslyn's `workspace/configuration`: inlay hints (parameters, types, lambdas), compiler diagnostics for the whole solution and analyzers for open files, completion from unimported namespaces. Checked against real Roslyn: parameter and type hints appear (off by default in Roslyn). Formatting already follows `.editorconfig` in Roslyn; nothing to map.                                                                                                                  |
| R5   | done           | Go to Definition on `List<T>` returns Roslyn's decompiled `<temp>/MetadataAsSource/…/List.cs`; the IDE now makes those files read-only, so they open read-only.                                                                                                                                                                                                                                                                                                                                      |
| R6   | done           | `language_smoke` `csharp` passes against real Roslyn 5.13 in ~30 s. Gap: Roslyn has no `source.organizeImports` action, so that check is off for C#.                                                                                                                                                                                                                                                                                                                                                 |
| P1   | done           | `unity/` is the package `dev.prefabric.unity` 0.1.0: Editor-only asmdef, Apache-2.0 LICENSE, `Third Party Notices.md` with the MIT texts, fixed `.meta` files. Installed as a `file:` package in Unity 6000.6.8f1; not yet installed by git URL, nor opened in 2022.3 or 6.3.                                                                                                                                                                                                                        |
| P2   | done           | Fork of `com.unity.ide.visualstudio` 2.0.29: SDK-style generator only, namespace `Prefabric.Unity.Editor` so it can sit next to the VS package. Diff of the output against the VS package's: `LangVersion` is Unity's (9.0) instead of `latest`, our generator version, analyzers and Unity's source generators added, package projects listed.                                                                                                                                                      |
| P3   | done           | `PrefabricEditor` (`IExternalCodeEditor`) finds `Prefabric*.app` in `/Applications` and `~/Applications`, `prefabric.exe` under LocalAppData and Program Files, `prefabric` in the usual Linux bins, and opens `prefabric <project> --line N --column M <file>`. The app now opens every path on its command line, so the file opens in that project's window (single instance forwards it). 3 EditMode tests pass in Unity. Double-click in Unity's Console against an installed app not yet tried. |
| P4   | done           | Microsoft.Unity.Analyzers 1.28.0 (SHA-512 checked against NuGet) in `Analyzers~`, so Unity's own compilation never loads it; added to every generated project. `dotnet build` of the generated solution: `UNT0001` on an empty `Update`, no IDE0051 on Unity messages, the `[SerializeField]` CS0649 false positive gone.                                                                                                                                                                            |
| P5   | done, untested | Under `UNITY_MSBUILD_ENABLED` the package writes `Library/Prefabric/MSBuild/Prefabric.Generated.props` and ships `Package.Build.props`/`.targets` (the VS package's mechanism, renamed). Not run: no Unity 7 alpha installed. Roslyn already prefers `<folder>.slnx`, the name Unity 7 uses.                                                                                                                                                                                                         |

## 1. Decisions

| #   | Decision                             | Status                        | Choice                                                                                                                                                                                                                 |
| --- | ------------------------------------ | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Q1  | Product name                         | decided                       | **Prefabric**. Bundle id `dev.prefabric.ide`, Unity package `dev.prefabric.unity`, binary `prefabric`. The app itself is renamed in task N2, before M1; until then it is still called OpenRider.                       |
| Q2  | Unity debugger                       | decided                       | **Fork** DotRush's Mono soft-debugger adapter (MIT) into our own repository and release it ourselves.                                                                                                                  |
| Q3  | .NET for the C# tools                | decided, refined by review V1 | **Bundle**: the IDE provides its own .NET **SDK** when the machine has no suitable SDK. It's a runtime plus MSBuild, which Roslyn needs to load projects, and it's fetched on first use, so the installer stays small. |
| Q4  | Oldest Unity version                 | decided                       | **2022.3 LTS** and newer. Main test targets: 2022.3, 6.3 LTS, and the 7.x alpha for CoreCLR.                                                                                                                           |
| Q5  | Unity package distribution           | default, not yet confirmed    | The app ships the package and installs it into `Packages/` as a `file:` reference, after asking first. A git URL works too. OpenUPM later.                                                                             |
| Q6  | Unity API data (event function list) | default, not yet confirmed    | Generate it from Unity's scripting reference. Don't copy `resharper-unity` data.                                                                                                                                       |
| Q7  | Order                                | decided                       | **M1 → M4 first; compare with Rider after (M5).** Each milestone checks its own speed targets, but nothing is compared with Rider until M5.                                                                            |

---

## 2. Name (Q1)

"OpenRider" uses JetBrains' "Rider" trademark, so it can't stay.

Also avoid "Unity", "IDEA", "JetBrains", "Rider" and "Visual Studio", in the name and in the package ids. Unity's trademark rules apply to products that imply they come from Unity.

| Name                          | Why it fits                                                                                 | What a quick web search found (Nov 2026; not a legal search)                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| **Prefabric** _(recommended)_ | "Prefab" plus "fabric": it reads as made for Unity without using Unity's name. Easy to say. | Found no IDE or developer tool by that name.                                                     |
| **Wisp**                      | Short and light, which matches the "fast, small" pitch.                                     | Only a hobby IDE and a small language; Wasp, a web framework, is a similar name.                 |
| **Kiln**                      | Where things get made, the build and forge idea.                                            | Kiln is an open-source 3D-asset tool for AI agents, used in game development too. Conflict risk. |
| ~~Glint~~                     | —                                                                                           | **Taken:** a commercial development workspace (Zide) and an Ember tool.                          |

**Chosen: Prefabric** (Q1).

- Product and window title: _Prefabric_.
- Bundle id: `dev.prefabric.ide`, if `prefabric.dev` is free.
- Unity package: `dev.prefabric.unity`.
- Binary: `prefabric`.

Before committing to it:

1. Search USPTO, EUIPO and WIPO in class 9 (software).
2. Check the domain, the GitHub organisation and the OpenUPM namespace.

---

## 3. Competitive picture (from v1 research, condensed)

- **Rider** (2026.2). It has the most complete Unity support:
  - Asset-aware Find Usages and Code Vision.
  - Unity inspections, performance-critical code highlighting, Burst/DOTS.
  - Shaders, asmdef, USS/UXML.
  - Debugging: Editor, player, Android, pausepoints.
  - Editor link: Play toolbar, log, test runner, Profiler integration.

  Its plugin, `JetBrains/resharper-unity`, is Apache-2.0.

  Its weak spots:
  - Users report 5–6 GB of RAM.
  - The setting "Automatically disable asset indexing for large solutions".
  - Slow startup and scrolling.
  - A paid licence for commercial use ($519 a year per seat for organisations).

- **Microsoft's Unity for VS Code** and its debugger are closed source. They only run in Microsoft's VS Code, and Cursor users fall back to DotRush.
- **DotRush** is MIT. It combines Roslyn with a Unity debugger, and it is our starting point for the debugger.
- **Unity's move to CoreCLR:** Unity 6.7 is the last version with Mono, and Unity 7 alphas have no Mono. Unity 7 adds an MSBuild pipeline that writes `.gen.csproj` and `.slnx` files. Standard .NET tools matter more after this change.

**Building blocks (all reusable):**

| Building block                                             | Licence |
| ---------------------------------------------------------- | ------- |
| Roslyn language server (`roslyn-language-server` on NuGet) | MIT     |
| Microsoft.Unity.Analyzers                                  | MIT     |
| `com.unity.ide.visualstudio` project generator             | MIT     |
| mono/debugger-libs, and DotRush's fork of it               | MIT     |
| netcoredbg                                                 | MIT     |
| Antaalt `shader-language-server` (HLSL only)               | MIT     |

**Not usable:** vsdbg and Microsoft's Unity debugger.

Sources are listed at the end.

---

## 4. What "better than Rider" means here

1. **Asset awareness that never switches off.** A Rust index of every scene and prefab, saved to disk, with no file-size or project-size limit.
2. **Things Rider doesn't have:**
   - a read-only GameObject tree for any scene or prefab, without Unity running;
   - a semantic diff for scenes and prefabs;
   - performance warnings that Unity's own compiler and CI also see.
3. **Light:** one .NET process (Roslyn). The rest is Rust and a webview.
4. **Free and open**, including commercial use.
5. **Ready for Unity 7 and CoreCLR** from the start.

---

## 5. Milestones

| Milestone | Goal                                                                        | Size            |
| --------- | --------------------------------------------------------------------------- | --------------- |
| **N**     | Name and rename                                                             | 2 d             |
| **M1**    | **Daily driver:** a Unity developer can do everyday C# work without Rider   | ≈ 56 d (11 ew)  |
| **M2**    | **Asset-aware:** "where is this used in the game?" on any project size      | ≈ 42 d (8.5 ew) |
| **M3**    | **Debug and Editor link:** debugging, Play/Pause, console, Unity tests      | ≈ 47 d (9.5 ew) |
| **M4**    | **Beyond Rider:** performance analyzer, scene diff, shaders, DOTS, profiler | ≈ 38 d (7.5 ew) |
| **M5**    | **Compare (later):** benchmarks against Rider and fixing what they show     | ≈ 15 d          |

**Total for N–M4: about 185 days, or 37 engineer-weeks.** That's roughly 9 months for one person, or about 5 months for two, using the parallel tracks in §7.

---

## 6. Task breakdown

- `d` = engineer-days.
- "Done when" lists what to check before the task's PR.
- Each task is one focused PR, with the checks from `CLAUDE.md` and evidence from running the app.

### N — Name (before M1)

| ID  | Task                                                                                                                | Where                                             | Dep. | d   | Done when                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- | ---- | --- | ------------------------------------------------------------------------ |
| N1  | Name chosen: Prefabric. Check trademarks in class 9, the domain, the GitHub organisation and the OpenUPM namespace. | —                                                 | —    | 1   | The name is recorded in Q1.                                              |
| N2  | Rename the product name, bundle id, binary name and docs. The plan's Unity package id changes too.                  | `tauri.conf.json`, `src/**`, `src-tauri/**`, docs | N1   | 1   | The app shows the new name. The data folder uses the new id. Tests pass. |

### M1 — Daily driver

**M1.1 Foundations** (moved here from the old M0)

| ID  | Task                                                                                                                                                                                                                                                 | Where                                      | Dep. | d   | Done when                                                                                            |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ | ---- | --- | ---------------------------------------------------------------------------------------------------- |
| F1  | `fixtures/unity-sample`, a Unity 2022.3 project that also opens in 6.3. Text serialization, 2 asmdefs, one prefab plus a variant plus a nested prefab, a UnityEvent, an animation event, an EditMode test, and generated csproj/sln files committed. | `fixtures/`                                | —    | 2   | It opens in Unity 2022.3 and 6.3 without errors. README explains how it was made.                    |
| F2  | `fixtures/csharp-app`: a `.slnx` and `.sln` with a library, a console app and an xUnit project.                                                                                                                                                      | `fixtures/`                                | —    | 0.5 | `dotnet build` and `dotnet test` pass.                                                               |
| F3  | **LSP startup hook** after `initialized`, plus custom requests and notifications for each server.                                                                                                                                                    | `ide-lsp/src/lib.rs:650`, `handler.rs:241` | —    | 3   | A unit test with a fake server sees the hook's notification. `jdtls` and `gopls` move onto the hook. |
| F4  | **NuGet tool source**: flat-container download, with the `packageHash` (sha512) from the registration API **required**. Pick the package for each OS and CPU.                                                                                        | `ide-tools/src/sources.rs`, `registry.rs`  | —    | 2   | It installs a test package. A tampered package is rejected.                                          |
| F5  | **Managed .NET SDK** (Q3):<br>• use the system SDK if it is new enough;<br>• otherwise install a private SDK into the tools directory, from Microsoft's release metadata, with its checksum checked.<br>All .NET tools get `DOTNET_ROOT`.            | `ide-tools`                                | F4   | 3   | It works on a machine with no SDK: macOS arm64, Windows x64, Linux x64.                              |
| F6  | **Trust gate.** In Restricted Mode, no MSBuild evaluation, Roslyn project loading, analyzers, source generators or `dotnet build`. See review R6.                                                                                                    | `ide-lsp` prepare hook, `ide-run`          | F3   | 1   | Test: an untrusted fixture with a malicious `.targets` file doesn't run it.                          |

**M1.2 Roslyn as the C# server**

| ID  | Task                                                                                                                                                                                                                                                      | Where                                     | Dep.   | d   | Done when                                                                                 |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------ | --- | ----------------------------------------------------------------------------------------- |
| R1  | A tool registry entry for `roslyn-language-server`, pinned to one version, installed from NuGet (F4).                                                                                                                                                     | `ide-tools/src/registry.rs`               | F4, F5 | 1   | It installs and starts with `--version` on all 3 OSes.                                    |
| R2  | `languages/csharp.toml`: Roslyn first, csharp-ls as the fallback, plus a setting to choose.                                                                                                                                                               | `languages/`, `ide-core/settings.rs`      | R1     | 1   | Switching the setting restarts the right server.                                          |
| R3  | **Project loading:**<br>• send `solution/open`, choosing the solution when there are several and preferring Unity's generated one;<br>• show progress in the status bar;<br>• handle `workspace/projectInitializationComplete` and restore notifications. | `ide-lsp` (F3 hook), status item          | F3, R2 | 3   | On `fixtures/csharp-app`, cross-project go-to works and diagnostics appear after loading. |
| R4  | Map our settings to Roslyn's: inlay hints, analyzer scope, formatting from `.editorconfig`.                                                                                                                                                               | `ide-sdk/src/apply.rs`                    | R3     | 2   | The settings change Roslyn's behaviour (checked in the app).                              |
| R5  | **Go to Definition into decompiled metadata**: Roslyn's metadata-as-source documents open read-only.                                                                                                                                                      | `ide-lsp`, editor virtual docs            | R3     | 2   | Go to Definition on `List<T>` opens the decompiled source.                                |
| R6  | Add C# to `language_smoke` with the pinned Roslyn version.                                                                                                                                                                                                | `crates/ide-host/tests/language_smoke.rs` | R3     | 1   | `cargo test … --ignored --exact csharp` passes.                                           |

**M1.3 The Unity package** (`unity/`, package `dev.prefabric.unity`)

| ID  | Task                                                                                                                                                                                                             | Where                                        | Dep. | d   | Done when                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- | ---- | --- | --------------------------------------------------------------------------------------------------- |
| P1  | Package skeleton: `package.json`, an Editor-only asmdef, LICENSE, and a NOTICE with MIT attribution for the forked code.                                                                                         | `unity/`                                     | N2   | 1   | It installs by git URL in 2022.3 and 6.3.                                                           |
| P2  | Fork the project generator from `com.unity.ide.visualstudio` 2.0.29 and remove the Visual Studio parts. It generates SDK-style csproj and `.sln`/`.slnx` files, plus Player projects.                            | `unity/Editor/`                              | P1   | 4   | The output loads in Roslyn (R3). Its diff against the VS package's output is only expected changes. |
| P3  | `IExternalCodeEditor`:<br>• find the installed IDE (macOS app bundle, Windows install path or registry, Linux);<br>• open `file:line:col` through the IDE's CLI, which forwards to the instance already running. | `unity/Editor/`, `src-tauri` CLI args        | P1   | 2   | Selectable in _External Tools_. A double-click on a console error opens the right line.             |
| P4  | Ship Microsoft.Unity.Analyzers (pinned) in the package and add it to every generated csproj.                                                                                                                     | `unity/`                                     | P2   | 1   | No "unused" warning on `Update`. The Unity warning `UNT0001` shows.                                 |
| P5  | Unity 7: when `UNITY_MSBUILD_ENABLED` is set, use Unity's `.gen.csproj`/`.slnx` files instead of generating our own.                                                                                             | `unity/`, `ide-workspace/unity.rs`           | P2   | 1   | Projects load on the Unity 7 alpha.                                                                 |
| P6  | IDE side: when the package is missing, offer to install it (Q5). Edit `Packages/manifest.json` after asking, copy the package from the app, and offer updates when versions differ.                              | `ide-workspace`, `src/features/project-tree` | P1   | 2   | On a fresh project: install, then Unity reloads and Prefabric appears in _External Tools_.          |
| P7  | Replace the "pick VS Code or Rider" notice with "Install package / Generate project files".                                                                                                                      | `ProjectView.tsx:785`                        | P6   | 0.5 | The new notice shows in English and Chinese, with screenshots.                                      |

**M1.4 Unity awareness without the Editor running**

| ID  | Task                                                                                                                                                                                                                          | Where                                     | Dep.   | d   | Done when                                                                                               |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------ | --- | ------------------------------------------------------------------------------------------------------- |
| U1  | **Unity message data** (Q6): a script that builds versioned JSON from Unity's scripting reference. It covers each message's signature, owning type, docs link, and whether it's a coroutine.                                  | `unity/data/`, script in `scripts/`       | —      | 3   | Covers MonoBehaviour, ScriptableObject, Editor, EditorWindow and AssetPostprocessor for 2022.3 and 6.3. |
| U2  | **Message completion**: in a class body whose base type (from Roslyn's type hierarchy) is a Unity type, typing `OnTrig…` inserts the full method.                                                                             | `src/features/` new `unity` feature       | U1, R3 | 3   | Works on the fixture, and doesn't suggest messages already implemented.                                 |
| U3  | **Event-function gutter icons**, with a tooltip and a docs link.                                                                                                                                                              | `unity` feature (editor contribution)     | U1     | 2   | Icons appear on `Update` and `OnTriggerEnter`, and on nothing else.                                     |
| U4  | **Unity-aware rename:**<br>• renaming a `MonoBehaviour`/`ScriptableObject` class also renames its file and moves the `.meta` file, keeping the GUID;<br>• renaming a serialized field offers `[FormerlySerializedAs("old")]`. | `unity` feature, `ide-workspace/unity.rs` | R3     | 3   | Test: after the rename the GUID is the same and the prefab still references the script.                 |

**M1.5 Unity file types**

| ID  | Task                                                                                                    | Where                         | Dep. | d   | Done when                                                   |
| --- | ------------------------------------------------------------------------------------------------------- | ----------------------------- | ---- | --- | ----------------------------------------------------------- |
| L1  | `.asmdef`/`.asmref`: JSON schema, plus Go to Definition on references by name and by `GUID:` reference. | `languages/`, `unity` feature | —    | 2   | Validation errors and navigation work on the fixture.       |
| L2  | USS as CSS with Unity property data. UXML as XML with Unity's generated `UIElementsSchema`.             | `languages/`                  | —    | 2   | Completion works for a USS property and for a UXML element. |
| L3  | Unity YAML (`%YAML 1.1` + `%TAG !u!`) opens read-only, with a "Edit anyway" action.                     | `languages/yaml.toml`, editor | —    | 0.5 | `.unity` and `.prefab` files open read-only.                |

**M1.6 Build, tests and run**

| ID  | Task                                                                                            | Where                            | Dep. | d   | Done when                                                          |
| --- | ----------------------------------------------------------------------------------------------- | -------------------------------- | ---- | --- | ------------------------------------------------------------------ |
| B1  | MSBuild diagnostics (`file(l,c): error CS…`) go to Problems.                                    | `ide-run` / `ide-build`          | —    | 2   | A broken fixture shows its errors in Problems, linked to the code. |
| B2  | A **TRX parser** that produces `TestEvent`s, for plain .NET. Unity tests come in E7.            | `src/features/run/testing/`      | F2   | 2   | The xUnit fixture shows a test tree, and "rerun failed" works.     |
| B3  | C# run gutter: `[Test]`, `[Fact]`, `[Theory]`, `[TestMethod]`, `Main` and top-level statements. | `src/features/run/targets.ts:44` | B2   | 2   | Gutter icons run and debug a single test.                          |
| B4  | `smoke/run` and `smoke/debug` for C#.                                                           | `smoke/`                         | B3   | 1   | Both smoke tests pass.                                             |

**M1 is done when:**

- every task's "Done when" holds;
- the fixture opens without warnings;
- the IDE is selectable in Unity 2022.3 and 6.3;
- completion, go-to and Unity analyzers work;
- rename keeps GUIDs;
- these targets hold on `fixtures/unity-sample` (our own targets, not compared with Rider):
  - first C# completion **≤ 15 s cold** and **≤ 5 s warm**;
  - IDE memory without Roslyn **≤ 400 MB**.

### M2 — Asset-aware intelligence

| ID  | Task                                                                                                                                                                                                                                                                               | Where                                                         | Dep.    | d   | Done when                                                                                                 |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- | ------- | --- | --------------------------------------------------------------------------------------------------------- |
| A1  | New crate **`ide-unity`**:<br>• its own redb store at `<localData>/unity-index/<hash>`, separate from `ide-index` so a change to the grammar fingerprint doesn't wipe it;<br>• starts only for Unity projects, after the editor is ready.                                          | `crates/ide-unity`, `ide-host/project.rs`                     | —       | 2   | The crate is wired in, with empty queries and an index-state event.                                       |
| A2  | **Scanner**:<br>• byte-level (memchr) reading of `--- !u!<class> &<fileId>` headers, `stripped`, `m_Script`, `m_Name`, `m_GameObject`, `m_Father`/`m_Children` and component lists;<br>• no YAML object tree, and no file-size limit.                                              | `ide-unity/src/scan.rs`                                       | A1      | 5   | Unit tests and a fuzz target. Never panics on any input.                                                  |
| A3  | **Prefabs**: prefab instances (`m_SourcePrefab`), variants, nested prefabs, and `m_Modifications` (property path, value, object reference) resolved against the source prefab.                                                                                                     | `ide-unity/src/prefab.rs`                                     | A2      | 5   | The fixture's variant and nested override resolve correctly.                                              |
| A4  | **UnityEvent bindings** (`m_PersistentCalls`: target and method) and **animation events** (`AnimationClip` `m_Events`).                                                                                                                                                            | `ide-unity`                                                   | A2      | 2   | The fixture's bindings are found.                                                                         |
| A5  | **GUID graph and query API**:<br>• asset↔GUID;<br>• script GUID → components → GameObject path;<br>• field → value/override;<br>• method → bindings.<br>Memory stays bounded: strings are interned, details load from the store on demand.                                         | `ide-unity/src/graph.rs`                                      | A3, A4  | 4   | Queries are tested on the fixture. Heap use is reported by a bench.                                       |
| A6  | **Incremental updates** from the `ide-vfs` watcher: re-scan a file only when its hash changes, and throttle bulk changes (branch switch, Unity reimport) into one batched rescan.                                                                                                  | `ide-unity`, `ide-host/project.rs:332`                        | A5      | 3   | Saving a prefab updates queries within 50 ms (median). A 5,000-file `git checkout` doesn't freeze the UI. |
| A7  | **Problem files:**<br>• binary-serialized projects: show a notice once, with "How to switch to Force Text";<br>• files with merge-conflict markers: skip and report.                                                                                                               | `ide-unity`, notifications                                    | A2      | 1   | Both cases are tested.                                                                                    |
| A8  | Criterion benches plus new budgets in `perf/budgets.json`: full scan of a generated 50,000-asset project, incremental update, query time, memory.                                                                                                                                  | `crates/ide-unity/benches`, `perf/`                           | A5      | 2   | Budgets are met: full scan ≤ 60 s on 8 cores, query ≤ 200 ms, index memory ≤ 300 MB.                      |
| A9  | **General Find Usages merge point**: extra reference providers return grouped results. Move the Java Spring config merge (`jdtls/configuration.rs:104`) onto it.                                                                                                                   | `ide-lsp/src/lib.rs:1401-1426`, `ops/lsp.rs`                  | —       | 3   | Java results don't change. A test provider's results appear.                                              |
| A10 | **Asset usages:**<br>• class → script GUID (from the `.cs` file's `.meta`) → scenes, prefabs and GameObjects, grouped like Rider's;<br>• method → bindings;<br>• serialized field → prefab overrides.<br>Asset results **arrive separately**, so they show before Roslyn finishes. | `ide-unity`, `src/features/code-insight/usages.ts`            | A5, A9  | 3   | Find Usages on the fixture's MonoBehaviour lists both code and asset usages, with asset usages first.     |
| A11 | **Code Vision:**<br>• "Used in N prefabs · M scenes" above a class;<br>• "Event handler in X" above a method;<br>• the Inspector value or "N overrides" next to a serialized field.<br>Also ask the server for LSP code lenses (`lspCodeLens`).                                    | `src/editor/lsp/codeLens.ts`, `unity` feature                 | A10     | 4   | The lenses show and update after a prefab is saved. Screenshots.                                          |
| A12 | **Search Everywhere providers can be added** (it has hardcoded tabs today).                                                                                                                                                                                                        | `src/features/search/everywhere/model.ts`, `contributions.ts` | —       | 2   | Existing tabs move onto the new API without behaviour changes. `perf` budgets still hold.                 |
| A13 | **Unity tab**: GameObjects by name across scenes and prefabs, and prefabs and assets by name, showing the hierarchy path.                                                                                                                                                          | `unity` feature                                               | A5, A12 | 2   | A query answers in ≤ 120 ms (the existing budget).                                                        |
| A14 | **Hierarchy preview**: open `.unity`/`.prefab` files as a read-only GameObject tree with components and overrides, with a toggle to raw YAML.                                                                                                                                      | custom editor kind (`contributions.ts`)                       | A5      | 4   | Works on the fixture, including a nested prefab. Screenshots.                                             |

**M2 is done when** a generated 50,000-asset project meets the A8 budgets, and Find Usages, Code Vision and search are correct on the fixture.

### M3 — Debugger and Editor link

**Debugger**

| ID  | Task                                                                                                                                                                                                                              | Where                                         | Dep.   | d   | Done when                                                               |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- | ------ | --- | ----------------------------------------------------------------------- |
| D1  | **Fork** DotRush's Mono debugger adapter (Q2) into our own repo:<br>• MIT notice kept;<br>• builds for each OS and CPU;<br>• releases have checksums;<br>• a tool registry entry.<br>No CI: releases are built by a local script. | new repo, `ide-tools/registry.rs`             | F5     | 3   | It installs and starts on all 3 OSes.                                   |
| D2  | **CoreCLR spike**: attach netcoredbg to the Unity 7 alpha Editor running CoreCLR. Write a one-page memo with a go/no-go for D9.                                                                                                   | —                                             | —      | 3   | The memo exists.                                                        |
| D3  | `AttachKind::UnityMono` with discovery of:<br>• **Editors**: debugger port `56000 + pid % 1000`;<br>• **players**: UDP multicast `225.0.0.222:54997`.                                                                             | `ide-dap/src/attach.rs`, `debugger/attach.ts` | D1     | 3   | Editors and a desktop player appear in the picker on macOS and Windows. |
| D4  | Devices: Android through `adb forward`, iOS through `iproxy`.                                                                                                                                                                     | `ide-dap`                                     | D3     | 3   | Attaches to an Android development build over USB.                      |
| D5  | Debug features:<br>• breakpoints, including conditional breakpoints and breakpoints in package code;<br>• source mapping for `Library/PackageCache`;<br>• stepping, evaluation, exception breakpoints.                            | adapter fork, `ide-dap`                       | D3     | 4   | A checklist on the fixture passes in 2022.3 and 6.3.                    |
| D6  | **Pausepoints**: pause Play mode (through the link) instead of stopping the thread.                                                                                                                                               | `ide-dap`, E3                                 | D5, E3 | 2   | Hitting a pausepoint pauses the Editor.                                 |
| D7  | `AttachKind::Dotnet`: netcoredbg attach for plain .NET, with a filtered process list.                                                                                                                                             | `ide-dap/src/attach.rs:104`                   | —      | 2   | Attaches to the running `csharp-app`.                                   |
| D8  | netcoredbg for macOS x64 and win-arm64, built from source by the release script.                                                                                                                                                  | `ide-tools/registry.rs:888`                   | —      | 2   | Installs on both.                                                       |
| D9  | Unity CoreCLR attach, only if D2 says go.                                                                                                                                                                                         | `ide-dap`                                     | D2     | 4   | The D5 checklist passes on Unity 7.                                     |

**Editor link**

| ID  | Task                                                                                                                                                                                                                                 | Where                                   | Dep.    | d   | Done when                                                      |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------- | ------- | --- | -------------------------------------------------------------- |
| E1  | **Protocol spec**:<br>• versioned JSON-RPC over loopback TCP;<br>• a per-session random token in `Library/` (git-ignored);<br>• the IDE connects to the Editor, so the Editor package never accepts connections from other machines. | `docs/`, `unity/`                       | —       | 1   | The spec has been reviewed.                                    |
| E2  | Package server: listener, dispatch onto the main thread, survives domain reloads.                                                                                                                                                    | `unity/Editor/Link/`                    | E1, P1  | 4   | The link stays up across script recompiles.                    |
| E3  | IDE client: connects and reconnects, a status bar item showing the play-mode state.                                                                                                                                                  | `src-tauri`/new module, `unity` feature | E1      | 3   | The status bar shows connected, playing, paused and compiling. |
| E4  | Play, Pause and Step toolbar; refresh assets on save, as a setting; Editor compile errors in Problems.                                                                                                                               | `unity` feature                         | E2, E3  | 3   | All work, with screenshots.                                    |
| E5  | **Unity Console** tool window: streamed logs, clickable stack traces, filters for log, warning and error, collapse, clear.                                                                                                           | `unity` feature                         | E3      | 3   | 10,000 log lines a minute don't lag the UI.                    |
| E6  | "Show in Unity": ping or select an asset or GameObject from M2 results.                                                                                                                                                              | `unity` feature                         | E3, A10 | 1   | Selecting a result selects it in Unity.                        |
| E7  | **Unity Test Runner** through `TestRunnerApi`: EditMode and PlayMode trees produce `TestEvent`s, with run and rerun from the gutter.                                                                                                 | `unity/`, `run/testing/`                | E3, B3  | 4   | The fixture's EditMode test runs from the gutter.              |
| E8  | "Attach to Editor and Play" in one click.                                                                                                                                                                                            | `unity` feature                         | D3, E4  | 1   | One click stops at a breakpoint in `Start`.                    |
| E9  | Security tests: a connection without the token is refused; nothing listens on non-loopback addresses; the token file is never committed (`Library/` is git-ignored).                                                                 | `unity/`, IDE client                    | E2      | 1   | Tests pass.                                                    |

**M3 is done when** the D5 checklist passes for the Editor and a desktop player on 2022.3 and 6.3 (and Unity 7 if D2 says go), and the link, console and test runner work on macOS and Windows.

### M4 — Beyond Rider (independent tasks, pick by demand)

| ID  | Task                                                                                                                                                                                                                                                                                                                                                                                    | Where                                                      | Dep. | d   | Done when                                                                                 |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ---- | --- | ----------------------------------------------------------------------------------------- |
| X1  | **Performance analyzer**: a Roslyn analyzer for .NET Standard 2.0, shipped in the package as a `RoslynAnalyzer`-labelled DLL.<br>• Rules for code inside `Update`, `FixedUpdate` and `LateUpdate` and methods they call in the same assembly.<br>• Flags `GetComponent`, `Find*`, `SendMessage`, LINQ, closures and allocations, string concatenation, and `Camera.main` before 2020.2. | `unity/Analyzers/`                                         | P4   | 8   | Each rule has tests. The warnings show both in Unity's console and in the IDE.            |
| X2  | UnityYAMLMerge as the merge tool for `.unity`/`.prefab`/`.asset` files, found from the installed Unity.                                                                                                                                                                                                                                                                                 | `ide-vcs`, VCS feature                                     | —    | 2   | A merge conflict in the fixture is resolved through it.                                   |
| X3  | **Scene and prefab diff** in terms of GameObjects, components and changed fields, using the `ide-unity` parser. Used in the commit view, history and PR review.                                                                                                                                                                                                                         | `ide-unity`, VCS feature                                   | A3   | 6   | The diff of the fixture's prefab edit shows "Player › Rigidbody.mass 1 → 2". Screenshots. |
| X4  | **ShaderLab**: a tree-sitter grammar with HLSL embedded in `HLSLPROGRAM`/`CGPROGRAM` blocks, plus highlighting and outline.                                                                                                                                                                                                                                                             | `crates/ide-language` grammars, `languages/shaderlab.toml` | —    | 6   | URP sample shaders parse without error nodes.                                             |
| X5  | HLSL server (`shader-language-server`) from GitHub releases, with include paths from `Packages/` and `Library/PackageCache`.                                                                                                                                                                                                                                                            | `ide-tools`, `languages/hlsl.toml`                         | X4   | 3   | URP includes resolve, and diagnostics show.                                               |
| X6  | DOTS/Burst: hints for Burst-compiled code (managed calls, allocations), plus baker and authoring templates.                                                                                                                                                                                                                                                                             | X1 analyzer, templates                                     | X1   | 5   | Tests and templates are in place.                                                         |
| X7  | **Profiler through the link**. The package reads profiler frames with Unity's `ProfilerDriver`/`FrameDataView` APIs; we don't parse Unity's undocumented `.data` capture files. Shows hot markers, with jump to source.                                                                                                                                                                 | `unity/Editor/Link/`, `unity` feature                      | E3   | 8   | A recorded Play session shows the top markers. A click opens the script.                  |

### M5 — Compare with Rider (after M4, as agreed)

| ID  | Task                                                                                                                                                | d   |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------- | --- |
| C1  | Add Rider and VS Code to `perf/compare/apps.toml`. New metrics: first completion, asset index ready, Find Usages latency, `footprint`-based memory. | 3   |
| C2  | Reference projects: medium (~5,000 assets) and large (30,000+), open-source with licences checked, cloned at bench time.                            | 2   |
| C3  | Run the comparison on macOS and Windows and publish the numbers in the repo.                                                                        | 3   |
| C4  | Fix idle CPU (median 14% of a core in the earlier benchmark), and fix whatever else C3 shows.                                                       | 5+  |
| C5  | A local "release check" script that runs the budgets and smoke tests and prints a table. It replaces CI, which is off by choice.                    | 2   |

---

## 7. Order, dependencies and parallel tracks

```
N ─► F1..F6 ─► R1..R6 ─► U2, U4 ─────────────► (M1 done)
       │        P1..P7 ─► P4 ─────────────────► X1 ─► X6
       │        U1, U3, L1..L3, B1..B4
       │
       └─► A1 ─► A2 ─► A3/A4 ─► A5 ─► A6..A8, A10 ─► A11, A13, A14, X3
                 A9, A12 (any time)
                                                    E1 ─► E2/E3 ─► E4..E9, D6, X7
                         D1 ─► D3 ─► D4, D5 ─► E8
                         D2 (any time, early) ─► D9
```

- **Track 1, C#/Unity code** (F, R, P, U, L, B, then X1, X6): the critical path.
- **Track 2, asset index** (A, then X3): it doesn't need Roslyn. Class → GUID mapping uses the file name and its `.meta` file, so this track can start on day 1 with a second engineer.
- **Spike D2** runs early. It's cheap and settles the biggest unknown: how Unity 7 debugs CoreCLR.
- With **one engineer**, go in this order:
  1. N and M1.1–M1.3;
  2. A1–A10;
  3. the rest of M1;
  4. the rest of M2;
  5. M3;
  6. M4.

  This puts the headline feature (asset-aware Find Usages) in front of users about halfway in, instead of last.

---

## 8. Review of the plan (findings applied in v2)

| #   | Finding                                                                                                                                                                                                                                               | Change made                                                                    |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| V1  | **A runtime is not enough.** Roslyn loads projects with MSBuild, which needs a .NET **SDK**. That's why Microsoft's Unity tools ask users to install one. Bundling only the runtime would leave Unity projects unloaded.                              | Q3 now means a private SDK fetched when needed. Task F5.                       |
| V2  | **Skipping M0 can't skip its foundations.** The fixtures, the LSP hook, the NuGet source, the Find Usages merge point and pluggable search are needed by M1–M2.                                                                                       | Moved into F1–F6, A9 and A12. Only the benchmarks moved to M5.                 |
| V3  | **M1 was underestimated** (6 ew). The task breakdown adds up to about 11 ew: the package fork, the SDK work and the trust gate weren't counted.                                                                                                       | Sizes updated. The total went from about 34 to about 37 ew.                    |
| V4  | **The plan only covered macOS.** Most Unity developers use Windows: Unity install paths, process discovery and debugger transports all differ there.                                                                                                  | Windows x64 is a required target in the "done when" checks (F5, D3, M3).       |
| V5  | M1's acceptance compared against Rider, which contradicts Q7.                                                                                                                                                                                         | Replaced with our own targets. Rider comparison only in M5.                    |
| V6  | **Security:** loading a C# project runs code. MSBuild targets, analyzers and source generators all execute when a project loads. Opening an untrusted repo would run that code, the same class of bug as the plugin `vcs_status` issue fixed earlier. | New task F6: a trust gate for all .NET project loading.                        |
| V7  | Unity's Profiler `.data` capture format is undocumented, so parsing it would be fragile.                                                                                                                                                              | X7 now reads frames through the Editor link and Unity's APIs.                  |
| V8  | Roslyn's Find Usages can be slow on big solutions. Merged results would wait for the slowest part.                                                                                                                                                    | A10 sends asset results separately, so they show first.                        |
| V9  | If the asset index shared `ide-index`'s store, any grammar change would wipe it and force a full Unity rescan.                                                                                                                                        | A1 gives it its own store.                                                     |
| V10 | Branch switches and Unity reimports touch thousands of files at once.                                                                                                                                                                                 | A6 batches bulk changes, with a UI-freeze check.                               |
| V11 | Unity tests can't run through `dotnet test`.                                                                                                                                                                                                          | B2 is for plain .NET; Unity tests go through the link (E7).                    |
| V12 | The name blocks the package id. Renaming the package after users install it is costly; it is now `dev.prefabric.unity`.                                                                                                                               | N1–N2 come before M1.3.                                                        |
| V13 | With no CI, budgets and smoke tests only run when someone remembers.                                                                                                                                                                                  | C5 adds a local release-check script. Each task's "Done when" names its check. |
| V14 | Forked MIT code (Unity's IDE package, DotRush's debugger) needs its notices kept.                                                                                                                                                                     | P1 and D1 include NOTICE and LICENSE work.                                     |

**Open risks:**

| Risk                                                              | Mitigation                                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Roslyn LSP is prerelease-only, and its protocol extensions change | Pin the version, cover it with R6, keep the csharp-ls fallback.           |
| How Unity 7 debugs CoreCLR is undocumented                        | D2 early; Mono and IL2CPP paths stay supported.                           |
| Unity 7's release date keeps moving                               | Only P5 and D9 depend on it.                                              |
| Unity YAML edge cases                                             | Fuzzing (A2), fixture coverage, skip and report broken files.             |
| netcoredbg on macOS arm64 is community-supported                  | Build it from source in our tool pipeline; test on Apple Silicon (D8).    |
| Private SDK download size, about 200 MB                           | Only when no suitable SDK is installed; show the size before downloading. |

## 9. Out of scope

- Unreal Engine.
- Mixed C#/C++ debugging and shader debugging.
- RenderDoc.
- AI features.
- Editing scenes or prefabs visually.
- Razor and Blazor.
- Writing our own C# analysis engine.

## Sources

- Rider Unity features: https://www.jetbrains.com/help/rider/2026.2/Features_Unity.html
- Rider's asset indexing setting: https://www.jetbrains.com/help/rider/2024.3/Settings_Unity_Engine.html
- Rider's Unity plugin source (Apache-2.0): https://github.com/JetBrains/resharper-unity
- Rider's free non-commercial licence: https://blog.jetbrains.com/blog/2024/10/24/webstorm-and-rider-are-now-free-for-non-commercial-use/
- Unity's path to CoreCLR: https://oceanviewgames.co.uk/blog/posts/unity-path-to-coreclr
- Unity's CoreCLR backend: https://docs.unity.com/en-us/engine/6000.7/manual/programming-environment/scripting-backends/coreclr
- Unity's MSBuild pipeline: https://docs.unity.com/en-us/engine/7000.0/manual/programming-environment/script-compilation/msbuild/unity
- `com.unity.ide.visualstudio` (MIT): https://github.com/needle-mirror/com.unity.ide.visualstudio
- Roslyn language server on NuGet: https://www.nuget.org/packages/roslyn-language-server
- roslyn.nvim, a client to learn from: https://github.com/seblyng/roslyn.nvim
- DotRush: https://github.com/JaneySprings/DotRush
- DotRush's Mono debugger fork: https://github.com/JaneySprings/Mono.Debugger
- Mono debugger libraries: https://github.com/mono/debugger-libs
- netcoredbg: https://github.com/Samsung/netcoredbg
- Microsoft.Unity.Analyzers: https://github.com/microsoft/Microsoft.Unity.Analyzers
- HLSL language server (shader-sense): https://github.com/Antaalt/shader-sense
- Unity for VS Code: https://marketplace.visualstudio.com/items?itemName=VisualStudioToolsForUnity.vstuc
- Cursor and the closed Unity extension: https://forum.cursor.com/t/unity-extension-does-not-support-anysphere-c-language-server/94361
- Name checks: Glint by Zide https://github.com/zide-software/Zide, Kiln https://mcpmarket.com/server/kiln-2
