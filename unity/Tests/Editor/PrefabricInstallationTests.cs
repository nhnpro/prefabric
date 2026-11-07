using System.IO;
using NUnit.Framework;

namespace Prefabric.Unity.Editor.Tests
{
	public class PrefabricInstallationTests
	{
		[Test]
		public void OpensTheProjectAndTheFileAtItsLine()
		{
			var file = Path.GetFullPath("Assets/Scripts/Player.cs");
			Assert.AreEqual(
				$"\"/work/Game\" --line 12 --column 5 \"{file}\"",
				PrefabricInstallation.OpenArguments("Assets/Scripts/Player.cs", 12, 5, "/work/Game"));
		}

		[Test]
		public void WithoutALineOpensTheFileAtItsStartAndWithoutAFileOnlyTheProject()
		{
			var file = Path.GetFullPath("a b.cs");
			Assert.AreEqual($"\"/work/Game\" \"{file}\"", PrefabricInstallation.OpenArguments("a b.cs", -1, -1, "/work/Game"));
			Assert.AreEqual($"\"/work/Game\" --line 3 --column 1 \"{file}\"", PrefabricInstallation.OpenArguments("a b.cs", 3, 0, "/work/Game"));
			Assert.AreEqual("\"/work/My Game\"", PrefabricInstallation.OpenArguments("", 0, 0, "/work/My Game"));
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
	}
}
