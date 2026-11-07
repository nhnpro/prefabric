# Prefabric for Unity (`dev.prefabric.unity`)

Makes Prefabric Unity's external code editor (Unity 2022.3 and newer):

- **Edit › Preferences › External Tools › External Script Editor** lists Prefabric. Double-clicking a
  script or a Console error opens it at that line in Prefabric's window for the project.
- Generates SDK-style `.csproj` files and a `.slnx` solution that Prefabric's C# server loads, with
  [Microsoft.Unity.Analyzers](https://github.com/microsoft/Microsoft.Unity.Analyzers) for the IDE
  only (Unity's own compilation is unchanged).
- With Unity's MSBuild pipeline (`UNITY_MSBUILD_ENABLED`), uses Unity's generated projects instead.

Prefabric installs this package for you. To add it by hand, add it to `Packages/manifest.json` as a
`file:` path to this folder, or by git URL with `?path=/unity`.

Project generation is a modified copy of Unity's Visual Studio Editor package; see
`Third Party Notices.md`.
