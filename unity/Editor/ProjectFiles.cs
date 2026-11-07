// Entry point for Prefabric's "Generate project files", run by the IDE in batch mode:
// Unity -batchmode -quit -projectPath <project> -executeMethod Prefabric.Unity.Editor.ProjectFiles.Generate

using UnityEditor;
using UnityEngine;

namespace Prefabric.Unity.Editor
{
	public static class ProjectFiles
	{
		public static void Generate()
		{
#if UNITY_MSBUILD_ENABLED
			MSBuildProjectIntegration.Update(PrefabricEditor.GenerationInstallation);
			Debug.Log("Prefabric: updated the MSBuild integration; Unity generates the projects.");
#else
			AssetDatabase.Refresh();
			PrefabricEditor.Generator.Sync();
			Debug.Log("Prefabric: generated " + PrefabricEditor.Generator.SolutionFile());
#endif
		}
	}
}
