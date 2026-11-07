// Prefabric installations: where the IDE is installed and how Unity opens files in it.
// Replaces the Visual Studio and VS Code installations of com.unity.ide.visualstudio.

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using Unity.CodeEditor;
using UnityEditor.PackageManager;
using IOPath = System.IO.Path;

namespace Prefabric.Unity.Editor
{
	internal class PrefabricInstallation
	{
		public const string DisplayName = "Prefabric";

		public string Name { get; private set; }
		public string Path { get; private set; }
		public Version Version { get; private set; }

		// Generated projects never ask for a newer C# than Unity's compiler accepts.
		public Version LatestLanguageVersionSupported => new Version(14, 0);

		public bool SupportsAnalyzers => true;

		/// <summary>Analyzers shipped in the package (Microsoft.Unity.Analyzers), outside Unity's asset
		/// import (`Analyzers~`), so they apply to the IDE only and never to Unity's own compilation.</summary>
		public string[] GetAnalyzers()
		{
			var package = PackageInfo.FindForAssembly(typeof(PrefabricInstallation).Assembly);
			if (package == null || string.IsNullOrEmpty(package.resolvedPath))
				return Array.Empty<string>();

			var directory = IOPath.Combine(package.resolvedPath, "Analyzers~");
			return Directory.Exists(directory)
				? Directory.GetFiles(directory, "*.dll", SearchOption.TopDirectoryOnly).OrderBy(p => p, StringComparer.Ordinal).ToArray()
				: Array.Empty<string>();
		}

		public static string PackageVersion()
		{
			return PackageInfo.FindForAssembly(typeof(PrefabricInstallation).Assembly)?.version ?? "0.0.0";
		}

		public CodeEditor.Installation ToCodeEditorInstallation()
		{
			return new CodeEditor.Installation { Name = Name, Path = Path };
		}

		/// <summary>The program to start: the app bundle's binary on macOS (through `open`), else the path itself.</summary>
		internal static bool IsCandidate(string path)
		{
			if (string.IsNullOrEmpty(path))
				return false;
#if UNITY_EDITOR_OSX
			return Directory.Exists(path) && Regex.IsMatch(IOPath.GetFileName(path.TrimEnd('/')), "^Prefabric.*\\.app$", RegexOptions.IgnoreCase);
#else
			var name = IOPath.GetFileNameWithoutExtension(path);
			return File.Exists(path) && name.Equals("prefabric", StringComparison.OrdinalIgnoreCase);
#endif
		}

		public static bool TryDiscover(string editorPath, out PrefabricInstallation installation)
		{
			installation = null;
			if (!IsCandidate(editorPath))
				return false;

			var version = ReadVersion(editorPath);
			installation = new PrefabricInstallation
			{
				Name = DisplayName + (version != null ? $" [{version.ToString(3)}]" : string.Empty),
				Path = editorPath,
				Version = version ?? new Version(),
			};
			return true;
		}

		private static Version ReadVersion(string editorPath)
		{
#if UNITY_EDITOR_OSX
			try
			{
				var plist = File.ReadAllText(IOPath.Combine(editorPath, "Contents", "Info.plist"));
				var match = Regex.Match(plist, "<key>CFBundleShortVersionString</key>\\s*<string>([0-9.]+)");
				if (match.Success && Version.TryParse(match.Groups[1].Value, out var version))
					return version;
			}
			catch (IOException)
			{
			}
#endif
			return null;
		}

		public static IEnumerable<PrefabricInstallation> Discover()
		{
			var candidates = new List<string>();
			var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
#if UNITY_EDITOR_OSX
			foreach (var applications in new[] { "/Applications", IOPath.Combine(home, "Applications") })
			{
				if (Directory.Exists(applications))
					candidates.AddRange(Directory.EnumerateDirectories(applications, "Prefabric*.app"));
			}
#elif UNITY_EDITOR_WIN
			var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
			candidates.Add(IOPath.Combine(localAppData, "Prefabric", "prefabric.exe"));
			candidates.Add(IOPath.Combine(localAppData, "Programs", "Prefabric", "prefabric.exe"));
			candidates.Add(IOPath.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Prefabric", "prefabric.exe"));
#else
			candidates.Add("/usr/bin/prefabric");
			candidates.Add("/usr/local/bin/prefabric");
			candidates.Add("/opt/Prefabric/prefabric");
			candidates.Add(IOPath.Combine(home, ".local", "bin", "prefabric"));
#endif
			foreach (var candidate in candidates.Distinct())
			{
				if (TryDiscover(candidate, out var installation))
					yield return installation;
			}
		}

		/// <summary>Opens the project folder, and `path` at `line`/`column` (1-based) when given. A running
		/// Prefabric takes the request (single instance) and shows the file in that project's window.</summary>
		public bool Open(string path, int line, int column, string projectDirectory)
		{
			var arguments = OpenArguments(path, line, column, projectDirectory);
#if UNITY_EDITOR_OSX
			// `open -n` starts the binary even when the app runs; that instance hands the request over and exits.
			ProcessRunner.Start(ProcessRunner.ProcessStartInfoFor("open", $"-n {Quote(Path)} --args {arguments}", redirect: false, shell: true));
#else
			ProcessRunner.Start(ProcessRunner.ProcessStartInfoFor(Path, arguments, redirect: false));
#endif
			return true;
		}

		internal static string OpenArguments(string path, int line, int column, string projectDirectory)
		{
			var arguments = Quote(projectDirectory);
			if (string.IsNullOrEmpty(path))
				return arguments;

			if (line > 0)
				arguments += $" --line {line} --column {Math.Max(1, column)}";
			return arguments + " " + Quote(IOPath.GetFullPath(path));
		}

		internal static string Quote(string value)
		{
			return "\"" + value.Replace("\"", "\\\"") + "\"";
		}
	}
}
