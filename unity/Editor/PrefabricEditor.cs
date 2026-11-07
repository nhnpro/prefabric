// The External Tools entry: Unity's code editor interface for Prefabric. Forked from
// com.unity.ide.visualstudio's VisualStudioEditor (MIT, see Third Party Notices.md).

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Unity.CodeEditor;
using UnityEditor;
using UnityEngine;

namespace Prefabric.Unity.Editor
{
	[InitializeOnLoad]
	public class PrefabricEditor : IExternalCodeEditor
	{
		private static readonly Dictionary<string, PrefabricInstallation> _installations = new Dictionary<string, PrefabricInstallation>();

		CodeEditor.Installation[] IExternalCodeEditor.Installations => _installations.Values.Select(i => i.ToCodeEditorInstallation()).ToArray();

		static PrefabricEditor()
		{
			if (!UnityInstallation.IsMainUnityEditorProcess)
				return;

#if !UNITY_MSBUILD_ENABLED
			MSBuildProjectIntegration.RemoveGeneratedProps();
#endif
			// A handful of fixed paths: cheap enough to check synchronously.
			foreach (var installation in PrefabricInstallation.Discover())
				_installations[FileUtility.GetAbsolutePath(installation.Path)] = installation;

			CodeEditor.Register(new PrefabricEditor());
		}

		internal static bool IsEnabled => CodeEditor.CurrentEditor is PrefabricEditor && UnityInstallation.IsMainUnityEditorProcess;

		/// <summary>The installation chosen in External Tools, when it is Prefabric.</summary>
		internal static PrefabricInstallation CurrentInstallation
		{
			get
			{
				if (!(CodeEditor.CurrentEditor is PrefabricEditor))
					return null;
				return TryGetInstallation(CodeEditor.CurrentEditorInstallation, out var installation) ? installation : null;
			}
		}

		private static bool TryGetInstallation(string editorPath, out PrefabricInstallation installation)
		{
			if (string.IsNullOrEmpty(editorPath))
			{
				installation = null;
				return false;
			}
			editorPath = FileUtility.GetAbsolutePath(editorPath);
			return _installations.TryGetValue(editorPath, out installation) || PrefabricInstallation.TryDiscover(editorPath, out installation);
		}

#if !UNITY_MSBUILD_ENABLED
		private static readonly IGenerator _generator = new SdkStyleProjectGeneration();
		internal static IGenerator Generator => _generator;
#endif

		public void Initialize(string editorInstallationPath)
		{
#if UNITY_MSBUILD_ENABLED
			if (TryGetInstallation(editorInstallationPath, out var installation))
				MSBuildProjectIntegration.Update(installation);
#endif
		}

		public bool TryGetInstallationForPath(string editorPath, out CodeEditor.Installation installation)
		{
			var result = TryGetInstallation(editorPath, out var found);
			installation = found?.ToCodeEditorInstallation() ?? default;
			return result;
		}

		public void OnGUI()
		{
			GUILayout.BeginHorizontal();
			GUILayout.FlexibleSpace();
			var style = new GUIStyle { richText = true, margin = new RectOffset(0, 4, 0, 0) };
			GUILayout.Label($"<size=10><color=grey>Prefabric package v{PrefabricInstallation.PackageVersion()} enabled</color></size>", style);
			GUILayout.EndHorizontal();

#if UNITY_MSBUILD_ENABLED
			EditorGUILayout.LabelField("Project generation is handled by Unity's MSBuild pipeline.");
#else
			EditorGUILayout.LabelField("Generate .csproj files for:");
			EditorGUI.indentLevel++;
			SettingsButton(ProjectGenerationFlag.Embedded, "Embedded packages", "");
			SettingsButton(ProjectGenerationFlag.Local, "Local packages", "");
			SettingsButton(ProjectGenerationFlag.Registry, "Registry packages", "");
			SettingsButton(ProjectGenerationFlag.Git, "Git packages", "");
			SettingsButton(ProjectGenerationFlag.BuiltIn, "Built-in packages", "");
			SettingsButton(ProjectGenerationFlag.LocalTarBall, "Local tarball", "");
			SettingsButton(ProjectGenerationFlag.Unknown, "Packages from unknown sources", "");
			SettingsButton(ProjectGenerationFlag.PlayerAssemblies, "Player projects", "For each player project generate an additional csproj with the name 'project-player.csproj'");
			var rect = EditorGUI.IndentedRect(EditorGUILayout.GetControlRect());
			rect.width = 252;
			if (GUI.Button(rect, "Regenerate project files"))
				_generator.Sync();
			EditorGUI.indentLevel--;
#endif
		}

#if !UNITY_MSBUILD_ENABLED
		private static void SettingsButton(ProjectGenerationFlag preference, string guiMessage, string toolTip)
		{
			var prevValue = _generator.AssemblyNameProvider.ProjectGenerationFlag.HasFlag(preference);
			var newValue = EditorGUILayout.Toggle(new GUIContent(guiMessage, toolTip), prevValue);
			if (newValue != prevValue)
				_generator.AssemblyNameProvider.ToggleProjectGeneration(preference);
		}
#endif

		public void SyncIfNeeded(string[] addedFiles, string[] deletedFiles, string[] movedFiles, string[] movedFromFiles, string[] importedFiles)
		{
#if UNITY_MSBUILD_ENABLED
			MSBuildProjectIntegration.Update(CurrentInstallation);
#else
			_generator.SyncIfNeeded(addedFiles.Union(deletedFiles).Union(movedFiles).Union(movedFromFiles), importedFiles);
#endif
		}

		public void SyncAll()
		{
#if UNITY_MSBUILD_ENABLED
			MSBuildProjectIntegration.Update(CurrentInstallation);
#else
			_generator.Sync();
#endif
		}

		public bool OpenProject(string path, int line, int column)
		{
			var editorPath = CodeEditor.CurrentEditorInstallation;
			if (!TryGetInstallation(editorPath, out var installation))
			{
				Debug.LogWarning($"Prefabric executable {editorPath} is not found. Please change your settings in Edit > Preferences > External Tools.");
				return false;
			}

#if UNITY_MSBUILD_ENABLED
			MSBuildProjectIntegration.Update(installation);
			var projectDirectory = Path.GetDirectoryName(MSBuildProjectIntegration.SolutionFilePath);
#else
			// Path is empty with "Open C# Project".
			if (!string.IsNullOrEmpty(path) && !_generator.IsSupportedFile(path))
				return false;
			_generator.Sync();
			var projectDirectory = _generator.ProjectDirectory;
#endif
			return installation.Open(path, line, column, projectDirectory);
		}
	}
}
