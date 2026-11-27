/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Unity Technologies.
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/
using System;
using System.IO;
using UnityEngine;

namespace Prefabric.Unity.Editor
{
	internal static class FileUtility
	{
		public const char WinSeparator = '\\';
		public const char UnixSeparator = '/';

		public static string GetAbsolutePath(string path)
		{
#if UNITY_6000_5_OR_NEWER
			return UnityEditor.FileUtil
				.PathToAbsolutePath(path)
				.NormalizePathSeparators();
#else
			return Path.GetFullPath(path);
#endif
		}

		public static string GetPackageAssetFullPath(params string[] components)
		{
			// Unity has special IO handling of Packages and will resolve those path to the right package location
			return GetAbsolutePath(Path.Combine("Packages", "dev.prefabric.unity", Path.Combine(components)));
		}

		public static string GetAssetFullPath(string asset)
		{
			var basePath = GetAbsolutePath(Path.Combine(Application.dataPath, ".."));
			return GetAbsolutePath(Path.Combine(basePath, NormalizePathSeparators(asset)));
		}

		public static string NormalizePathSeparators(this string path)
		{
			if (string.IsNullOrEmpty(path))
				return path;

			if (Path.DirectorySeparatorChar == WinSeparator)
				path = path.Replace(UnixSeparator, WinSeparator);
			if (Path.DirectorySeparatorChar == UnixSeparator)
				path = path.Replace(WinSeparator, UnixSeparator);

			if (Path.DirectorySeparatorChar == WinSeparator)
			{
				var unc = path.StartsWith(@"\\", StringComparison.Ordinal);
				var body = unc ? path.Substring(2) : path;
				while (body.Contains(@"\\"))
					body = body.Replace(@"\\", @"\");
				return unc ? @"\\" + body : body;
			}

			return path;
		}

		public static string NormalizeWindowsToUnix(this string path)
		{
			if (string.IsNullOrEmpty(path))
				return path;

			return path.Replace(WinSeparator, UnixSeparator);
		}

		internal static bool IsFileInProjectRootDirectory(string fileName)
		{
			var relative = MakeRelativeToProjectPath(fileName);
			if (string.IsNullOrEmpty(relative))
				return false;

			return relative == Path.GetFileName(relative);
		}

		internal static bool IsMSBuildGeneratedProject(string path)
		{
			return !string.IsNullOrEmpty(path) && path.EndsWith(".gen.csproj", StringComparison.OrdinalIgnoreCase);
		}

		public static string MakeAbsolutePath(this string path)
		{
			if (string.IsNullOrEmpty(path)) { return string.Empty; }
			return Path.IsPathRooted(path) ? path : GetAbsolutePath(path);
		}

		// returns null if outside of the project scope
		internal static string MakeRelativeToProjectPath(string fileName)
		{
			if (string.IsNullOrEmpty(fileName))
				return null;

			var basePath = GetAbsolutePath(Path.Combine(Application.dataPath, ".."))
				.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
			fileName = NormalizePathSeparators(fileName);

			try
			{
				fileName = Path.IsPathRooted(fileName)
					? GetAbsolutePath(fileName)
					: GetAbsolutePath(Path.Combine(basePath, fileName));
			}
			catch (Exception e) when (e is ArgumentException || e is NotSupportedException || e is PathTooLongException)
			{
				return null;
			}

			var comparison =
#if UNITY_EDITOR_WIN || UNITY_EDITOR_OSX
				StringComparison.OrdinalIgnoreCase;
#else
				StringComparison.Ordinal;
#endif
			var basePrefix = basePath + Path.DirectorySeparatorChar;
			if (!fileName.StartsWith(basePrefix, comparison))
				return null;

			return fileName
				.Substring(basePrefix.Length)
				.Trim(Path.DirectorySeparatorChar);
		}

		internal static void SafeDelete(string file)
		{
			try
			{
				if (File.Exists(file))
					File.Delete(file);
			}
			catch (Exception e) when (e is IOException || e is UnauthorizedAccessException)
			{
				// ignore
			}
		}
	}
}
