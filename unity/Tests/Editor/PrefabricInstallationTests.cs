using System.IO;
using NUnit.Framework;

namespace Prefabric.Unity.Editor.Tests
{
	public class PrefabricInstallationTests
	{
		[Test]
		public void OpensTheProjectAndTheFileAtItsLine()
		{
			var file = Path.GetFullPath(Path.Combine("/work/Game", "Assets/Scripts/Player.cs"));
			Assert.AreEqual(
				$"\"/work/Game\" --line 12 --column 5 \"{file}\"",
				PrefabricInstallation.OpenArguments("Assets/Scripts/Player.cs", 12, 5, "/work/Game"));
		}

		[Test]
		public void WithoutALineOpensTheFileAtItsStartAndWithoutAFileOnlyTheProject()
		{
			var file = Path.GetFullPath(Path.Combine("/work/Game", "a b.cs"));
			Assert.AreEqual($"\"/work/Game\" \"{file}\"", PrefabricInstallation.OpenArguments("a b.cs", -1, -1, "/work/Game"));
			Assert.AreEqual($"\"/work/Game\" --line 3 --column 1 \"{file}\"", PrefabricInstallation.OpenArguments("a b.cs", 3, 0, "/work/Game"));
			Assert.AreEqual("\"/work/My Game\"", PrefabricInstallation.OpenArguments("", 0, 0, "/work/My Game"));
		}

		[Test]
		public void QuotesTrailingBackslashesAndQuotesForWindowsCrt()
		{
			Assert.AreEqual("\"C:\\\\\"", PrefabricInstallation.Quote("C:\\"));
			Assert.AreEqual("\"C:\\Dir\\\\\"", PrefabricInstallation.Quote("C:\\Dir\\"));
			Assert.AreEqual("\"a\\\\\\\"b\"", PrefabricInstallation.Quote("a\\\"b"));
		}

		[Test]
		public void OnlyPrefabricInstallationsAreCandidates()
		{
			var dir = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
#if UNITY_EDITOR_OSX
			var app = Path.Combine(dir, "Prefabric.app");
			var other = Path.Combine(dir, "Visual Studio Code.app");
			Directory.CreateDirectory(app);
			Directory.CreateDirectory(other);
#else
			Directory.CreateDirectory(dir);
			var app = Path.Combine(dir, "prefabric" + (System.Environment.OSVersion.Platform == System.PlatformID.Win32NT ? ".exe" : ""));
			var other = Path.Combine(dir, "code");
			File.WriteAllText(app, "");
			File.WriteAllText(other, "");
#endif
			try
			{
				Assert.IsTrue(PrefabricInstallation.TryDiscover(app, out var installation));
				Assert.AreEqual(app, installation.Path);
				Assert.IsFalse(PrefabricInstallation.TryDiscover(other, out _));
				Assert.IsFalse(PrefabricInstallation.TryDiscover(Path.Combine(dir, "missing"), out _));
			}
			finally
			{
				Directory.Delete(dir, true);
			}
		}

		[Test]
		public void MakeRelativeToProjectPathRespectsDirectoryBoundaryAndCanonicalizes()
		{
			Assert.IsNull(FileUtility.MakeRelativeToProjectPath(null));
			Assert.IsNull(FileUtility.MakeRelativeToProjectPath(""));
			var projectDir = Path.GetFullPath(Path.Combine(UnityEngine.Application.dataPath, ".."));
			Assert.IsNull(FileUtility.MakeRelativeToProjectPath(projectDir + "Extra.csproj"));
			Assert.AreEqual("Assembly-CSharp.csproj", FileUtility.MakeRelativeToProjectPath("./Assembly-CSharp.csproj"));
			Assert.IsTrue(FileUtility.IsFileInProjectRootDirectory("./Assembly-CSharp.csproj"));
		}

#if !UNITY_MSBUILD_ENABLED
		[Test]
		public void XmlFilenameEscapesMsBuildAndXmlMetacharacters()
		{
			Assert.AreEqual(
				"R&amp;D%2520%3b%24%40%28%29%2a%3f.dll",
				ProjectGeneration.XmlFilename("R&D%20;$@()*?.dll"));
		}

		[Test]
		public void SolutionParserSkipsProjectElementsWithoutPathAttribute()
		{
			var sln = SolutionParser.ParseSolutionXmlContent("<Solution><Project /><Project Path=\"Game.csproj\" /></Solution>");
			Assert.AreEqual(1, sln.Projects.Length);
			Assert.AreEqual("Game.csproj", sln.Projects[0].FileName);
		}

		[Test]
		public void LatestLanguageVersionParsesMajorOnlyVersions()
		{
			var assembly = new UnityEditor.Compilation.Assembly
			{
				compilerOptions = new UnityEditor.Compilation.ScriptCompilerOptions { LanguageVersion = "10" },
			};
			Assert.AreEqual(new System.Version(10, 0), UnityInstallation.LatestLanguageVersionSupported(assembly));
		}
#endif
	}
}
