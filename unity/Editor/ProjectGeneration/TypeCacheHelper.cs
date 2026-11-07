/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Unity Technologies.
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

#if !UNITY_MSBUILD_ENABLED
using SR = System.Reflection;

using System.Collections.Generic;
using System.Linq;
using UnityEditor;

namespace Prefabric.Unity.Editor {

	internal class TypeCacheHelper
	{
		internal static IEnumerable<SR.MethodInfo> GetPostProcessorCallbacks(string name)
		{
			return TypeCache
				.GetTypesDerivedFrom<AssetPostprocessor>()
				.Where(t => t.Assembly.GetName().Name != "SyntaxTree.VisualStudio.Unity.Bridge") // never call into the bridge if loaded with the package
				.Select(t => t.GetMethod(name, SR.BindingFlags.Public | SR.BindingFlags.NonPublic | SR.BindingFlags.Static))
				.Where(m => m != null);
		}
	}

}
#endif
