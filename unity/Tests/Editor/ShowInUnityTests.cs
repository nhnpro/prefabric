using System.IO;
using NUnit.Framework;
using Prefabric.Unity.Editor.Link;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace Prefabric.Unity.Editor.Tests
{
	/// <summary>"Show in Unity" (plan task E6): objects found by the file ID in their YAML.</summary>
	public class ShowInUnityTests
	{
		const string Folder = "Assets/PrefabricShowTests";

		[SetUp]
		public void CreateFolder() => Directory.CreateDirectory(Folder);

		[TearDown]
		public void DeleteFolder()
		{
			EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
			AssetDatabase.DeleteAsset(Folder);
		}

		static string FileId(Object o) => ((long)GlobalObjectId.GetGlobalObjectIdSlow(o).targetObjectId).ToString();

		[Test]
		public void SelectsAGameObjectOfAnOpenScene()
		{
			var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
			var go = new GameObject("Target");
			var path = Folder + "/Show.unity";
			EditorSceneManager.SaveScene(scene, path);
			Assert.IsTrue(Link.Link.SelectObject(path, FileId(go)));
			Assert.AreEqual(go, Selection.activeGameObject);
		}

		[Test]
		public void SelectsTheGameObjectOfAComponent()
		{
			var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
			var go = new GameObject("Owner");
			var body = go.AddComponent<BoxCollider>();
			var path = Folder + "/Component.unity";
			EditorSceneManager.SaveScene(scene, path);
			Assert.IsTrue(Link.Link.SelectObject(path, FileId(body)));
			Assert.AreEqual(go, Selection.activeGameObject);
		}

		[Test]
		public void SelectsAnObjectInsideAPrefab()
		{
			var root = new GameObject("Root");
			new GameObject("Child").transform.SetParent(root.transform);
			var path = Folder + "/Show.prefab";
			var prefab = PrefabUtility.SaveAsPrefabAsset(root, path);
			Object.DestroyImmediate(root);
			var child = prefab.transform.GetChild(0).gameObject;
			Assert.IsTrue(Link.Link.SelectObject(path, FileId(child)));
			Assert.AreEqual(child, Selection.activeObject);
		}

		[Test]
		public void PingsTheSceneOfAnObjectWhoseSceneIsClosed()
		{
			var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
			var id = FileId(new GameObject("Elsewhere"));
			var path = Folder + "/Closed.unity";
			EditorSceneManager.SaveScene(scene, path);
			EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
			Assert.IsFalse(Link.Link.SelectObject(path, id));
			Assert.AreEqual(AssetDatabase.LoadMainAssetAtPath(path), Selection.activeObject);
		}
	}
}
