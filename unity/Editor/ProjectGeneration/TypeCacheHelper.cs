/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Unity Technologies.
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

#if !UNITY_MSBUILD_ENABLED
using SR = System.Reflection;

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;

namespace Prefabric.Unity.Editor {

	internal class TypeCacheHelper
	{
		private static readonly ConcurrentDictionary<string, SR.MethodInfo[]> _cache = new ConcurrentDictionary<string, SR.MethodInfo[]>(StringComparer.Ordinal);

		internal static IEnumerable<SR.MethodInfo> GetPostProcessorCallbacks(string name, params Type[] parameterTypes)
		{
			var key = parameterTypes.Length == 0 ? name : name + "(" + string.Join(",", parameterTypes.Select(t => t.FullName)) + ")";
			return _cache.GetOrAdd(key, _ => TypeCache
				.GetTypesDerivedFrom<AssetPostprocessor>()
				.Where(t => t.Assembly.GetName().Name != "SyntaxTree.VisualStudio.Unity.Bridge") // never call into the bridge if loaded with the package
				.SelectMany(t => t.GetMethods(SR.BindingFlags.Public | SR.BindingFlags.NonPublic | SR.BindingFlags.Static))
				.Where(m => m.Name == name && ParametersMatch(m.GetParameters(), parameterTypes))
				.ToArray());
		}

		private static bool ParametersMatch(SR.ParameterInfo[] parameters, Type[] expected)
		{
			if (parameters.Length != expected.Length)
				return false;
			for (var i = 0; i < expected.Length; i++)
			{
				if (parameters[i].ParameterType != expected[i])
					return false;
			}
			return true;
		}
	}

}
#endif
