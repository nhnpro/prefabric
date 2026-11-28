/** Built-in file templates for the "New ▸" menu, with the includes and code templates they `#parse`. */
import type { FileTemplate, TemplateFragment } from "./model";

/** Includes tab: fragments file templates pull in with `#parse("…")`. */
export const BUILTIN_INCLUDES: readonly TemplateFragment[] = [{ name: "File Header", body: "" }];

/** Opens empty: the notebook editor adds the first cell. */
const JUPYTER_NOTEBOOK_BODY = `{
 "cells": [],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "name": "python"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
`;

/** Starts with a Markdown heading named after the file. */
const JUPYTER_TITLED_NOTEBOOK_BODY = `{
 "cells": [
  {
   "cell_type": "markdown",
   "id": "cell-title",
   "metadata": {},
   "source": [
    "# $NAME$"
   ]
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "name": "python"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
`;

/** Code tab: the members the built-in test templates generate. */
export const BUILTIN_CODE_TEMPLATES: readonly TemplateFragment[] = [
  {
    name: "JUnit5 Test Method",
    description: "Java Class › JUnit 5 Test",
    body: "@Test\nvoid test() {\n    $END$\n}\n",
  },
  {
    name: "Kotlin JUnit5 Test Method",
    description: "Kotlin Class/File › JUnit 5 Test",
    body: "@Test\nfun test() {\n    $END$\n}\n",
  },
  {
    name: "Go Test Function",
    description: "Go File › Test file",
    body: "func Test$NAME$(t *testing.T) {\n\t$END$\n}\n",
  },
  {
    name: "Zig Test Function",
    description: "Zig File › Test file",
    body: 'test "$NAME$" {\n    $END$\n}\n',
  },
  {
    name: "Rust Test Function",
    description: "Rust File › Test module",
    body: "#[test]\nfn it_works() {\n    $END$\n}\n",
  },
  {
    name: "Vitest Test",
    description: "TypeScript File › Test",
    body: 'it("works", () => {\n  $END$\n});\n',
  },
  {
    name: "PHPUnit Test Method",
    description: "PHP Class › PHPUnit Test",
    body: "public function testSomething(): void\n{\n    $END$\n}\n",
  },
  {
    name: "pytest Test Function",
    description: "Python File › Test (pytest)",
    body: "def test_$NAME$():\n    $END$\n",
  },
];

export const BUILTIN_FILE_TEMPLATES: readonly FileTemplate[] = [
  {
    id: "file",
    name: "File",
    icon: "file",
    extension: "",
    nameStyle: "asis",
    kinds: [{ id: "file", label: "File", body: "$END$" }],
  },
  {
    id: "rust",
    name: "Rust File",
    icon: "file-rust",
    extension: "rs",
    nameStyle: "snake",
    kinds: [
      { id: "module", label: "Module", body: "$END$\n" },
      { id: "struct", label: "Struct", body: "pub struct $NAME$ {\n    $END$\n}\n" },
      {
        id: "test",
        label: "Test module",
        body: '#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #parse("Rust Test Function")\n}\n',
      },
    ],
  },
  {
    id: "typescript",
    name: "TypeScript File",
    icon: "file-ts",
    extension: "ts",
    nameStyle: "asis",
    kinds: [
      { id: "file", label: "File", body: "$END$\n" },
      { id: "class", label: "Class", body: "export class $NAME$ {\n  $END$\n}\n" },
      {
        id: "test",
        label: "Test",
        body: 'import { describe, expect, it } from "vitest";\n\ndescribe("$NAME$", () => {\n  #parse("Vitest Test")\n});\n',
      },
    ],
  },
  {
    id: "tsx",
    name: "React Component",
    icon: "file-tsx",
    extension: "tsx",
    nameStyle: "pascal",
    kinds: [
      {
        id: "component",
        label: "Function component",
        body: "export interface $NAME$Props {}\n\nexport function $NAME$(props: $NAME$Props) {\n  return <div>$END$</div>;\n}\n",
      },
      {
        id: "styled",
        label: "Component with CSS module",
        body: 'import styles from "./$NAME$.module.css";\n\nexport function $NAME$() {\n  return <div className={styles.root}>$END$</div>;\n}\n',
      },
    ],
  },
  {
    id: "javascript",
    name: "JavaScript File",
    icon: "file-js",
    extension: "js",
    nameStyle: "asis",
    kinds: [{ id: "file", label: "File", body: "$END$\n" }],
  },
  {
    id: "python",
    name: "Python File",
    icon: "file-python",
    extension: "py",
    nameStyle: "snake",
    kinds: [
      { id: "module", label: "Module", body: '"""$NAME$ module."""\n\n$END$\n' },
      {
        id: "class",
        label: "Class",
        body: "class $NAME$:\n    def __init__(self):\n        $END$\n",
      },
      { id: "test", label: "Test (pytest)", body: '#parse("pytest Test Function")\n' },
      { id: "script", label: "Script", body: 'def main():\n    $END$\n\n\nif __name__ == "__main__":\n    main()\n' },
    ],
  },
  {
    id: "jupyter",
    name: "Jupyter Notebook",
    icon: "file-jupyter",
    extension: "ipynb",
    nameStyle: "asis",
    kinds: [
      { id: "notebook", label: "Notebook", body: JUPYTER_NOTEBOOK_BODY },
      { id: "titled", label: "Notebook with a title", body: JUPYTER_TITLED_NOTEBOOK_BODY },
    ],
  },
  {
    id: "go",
    name: "Go File",
    icon: "file-go",
    extension: "go",
    nameStyle: "snake",
    packageStyle: "go",
    kinds: [
      { id: "file", label: "Empty file", body: "package $PACKAGE$\n\n$END$\n" },
      {
        id: "test",
        label: "Test file",
        body: 'package $PACKAGE$\n\nimport "testing"\n\n#parse("Go Test Function")\n',
      },
      { id: "main", label: "Main", body: "package main\n\nfunc main() {\n\t$END$\n}\n" },
    ],
  },
  {
    id: "go-mod",
    name: "Go Module",
    icon: "file-gomod",
    extension: "mod",
    nameStyle: "asis",
    defaultName: "go",
    kinds: [{ id: "module", label: "Module", body: "module $PROJECT_NAME$\n\ngo 1.20\n" }],
  },
  {
    id: "zig",
    name: "Zig File",
    icon: "file-zig",
    extension: "zig",
    nameStyle: "snake",
    kinds: [
      { id: "file", label: "Empty file", body: "$END$\n" },
      { id: "main", label: "Main", body: "pub fn main() !void {\n    $END$\n}\n" },
      { id: "test", label: "Test file", body: '#parse("Zig Test Function")\n' },
      { id: "struct", label: "Struct", body: "pub const $NAME$ = struct {\n    $END$\n};\n" },
    ],
  },
  {
    id: "php",
    name: "PHP File",
    icon: "file-php",
    extension: "php",
    nameStyle: "asis",
    kinds: [{ id: "file", label: "File", body: "<?php\n\n$END$\n" }],
  },
  {
    id: "php-class",
    name: "PHP Class",
    icon: "class",
    extension: "php",
    nameStyle: "pascal",
    packageStyle: "php",
    kinds: [
      {
        id: "class",
        label: "Class",
        body: '<?php\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\nclass $NAME$\n{\n    $END$\n}\n',
      },
      {
        id: "interface",
        label: "Interface",
        body: '<?php\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\ninterface $NAME$\n{\n    $END$\n}\n',
      },
      {
        id: "trait",
        label: "Trait",
        body: '<?php\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\ntrait $NAME$\n{\n    $END$\n}\n',
      },
      {
        id: "enum",
        label: "Enum",
        body: '<?php\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\nenum $NAME$\n{\n    $END$\n}\n',
      },
      {
        id: "test",
        label: "PHPUnit Test",
        body: '<?php\n\nnamespace $PACKAGE$;\n\nuse PHPUnit\\Framework\\TestCase;\n\n#parse("File Header")\nfinal class $NAME$ extends TestCase\n{\n    #parse("PHPUnit Test Method")\n}\n',
      },
    ],
  },
  {
    id: "java",
    name: "Java Class",
    icon: "class",
    extension: "java",
    nameStyle: "pascal",
    packageStyle: "jvm",
    dottedPackages: true,
    kinds: [
      {
        id: "class",
        label: "Class",
        body: 'package $PACKAGE$;\n\n#parse("File Header")\npublic class $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "interface",
        label: "Interface",
        body: 'package $PACKAGE$;\n\n#parse("File Header")\npublic interface $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "enum",
        label: "Enum",
        body: 'package $PACKAGE$;\n\n#parse("File Header")\npublic enum $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "record",
        label: "Record",
        body: 'package $PACKAGE$;\n\n#parse("File Header")\npublic record $NAME$($END$) {\n}\n',
      },
      {
        id: "annotation",
        label: "Annotation",
        body: 'package $PACKAGE$;\n\n#parse("File Header")\npublic @interface $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "test",
        label: "JUnit 5 Test",
        body: 'package $PACKAGE$;\n\nimport org.junit.jupiter.api.Test;\n\nimport static org.junit.jupiter.api.Assertions.*;\n\n#parse("File Header")\nclass $NAME$ {\n\n    #parse("JUnit5 Test Method")\n}\n',
      },
    ],
  },
  {
    id: "csharp",
    name: "C# Class",
    icon: "class",
    extension: "cs",
    nameStyle: "pascal",
    packageStyle: "csharp",
    kinds: (
      [
        ["class", "Class", "class"],
        ["interface", "Interface", "interface"],
        ["struct", "Struct", "struct"],
        ["enum", "Enum", "enum"],
        ["record", "Record", "record"],
      ] as const
    ).map(([id, label, keyword]) => ({
      id,
      label,
      body: `namespace $PACKAGE$;\n\n#parse("File Header")\npublic ${keyword} $NAME$\n{\n    $END$\n}\n`,
    })),
  },
  {
    id: "kotlin",
    name: "Kotlin Class/File",
    icon: "class",
    extension: "kt",
    nameStyle: "pascal",
    packageStyle: "jvm",
    dottedPackages: true,
    kinds: [
      {
        id: "class",
        label: "Class",
        body: 'package $PACKAGE$\n\n#parse("File Header")\nclass $NAME$ {\n    $END$\n}\n',
      },
      { id: "file", label: "File", body: "package $PACKAGE$\n\n$END$\n" },
      {
        id: "interface",
        label: "Interface",
        body: 'package $PACKAGE$\n\n#parse("File Header")\ninterface $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "data",
        label: "Data class",
        body: 'package $PACKAGE$\n\n#parse("File Header")\ndata class $NAME$($END$)\n',
      },
      {
        id: "enum",
        label: "Enum class",
        body: 'package $PACKAGE$\n\n#parse("File Header")\nenum class $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "object",
        label: "Object",
        body: 'package $PACKAGE$\n\n#parse("File Header")\nobject $NAME$ {\n    $END$\n}\n',
      },
      {
        id: "test",
        label: "JUnit 5 Test",
        body: 'package $PACKAGE$\n\nimport org.junit.jupiter.api.Test\nimport org.junit.jupiter.api.Assertions.*\n\n#parse("File Header")\nclass $NAME$ {\n\n    #parse("Kotlin JUnit5 Test Method")\n}\n',
      },
    ],
  },
  {
    id: "html",
    name: "HTML File",
    icon: "file-html",
    extension: "html",
    nameStyle: "asis",
    kinds: [
      {
        id: "html5",
        label: "HTML 5 file",
        body: '<!doctype html>\n<html lang="en">\n<head>\n  <meta charset="UTF-8">\n  <meta name="viewport" content="width=device-width, initial-scale=1">\n  <title>$NAME$</title>\n</head>\n<body>\n$END$\n</body>\n</html>\n',
      },
      { id: "fragment", label: "Fragment", body: "$END$\n" },
    ],
  },
  {
    id: "markdown",
    name: "Markdown File",
    icon: "file-md",
    extension: "md",
    nameStyle: "asis",
    kinds: [{ id: "md", label: "Markdown file", body: "# $NAME$\n\n$END$\n" }],
  },
  {
    // IDEA's "Stylesheet": one dialog, the kind picks the language.
    id: "stylesheet",
    name: "Stylesheet",
    icon: "file-css",
    extension: "css",
    nameStyle: "asis",
    kinds: [
      { id: "css", label: "CSS File", body: "$END$\n" },
      { id: "scss", label: "SCSS File", body: "$END$\n", extension: "scss" },
      { id: "sass", label: "Sass File", body: "$END$\n", extension: "sass" },
      { id: "less", label: "Less File", body: "$END$\n", extension: "less" },
    ],
  },
  {
    id: "json",
    name: "JSON File",
    icon: "file-json",
    extension: "json",
    nameStyle: "asis",
    kinds: [{ id: "json", label: "JSON file", body: "{\n  $END$\n}\n" }],
  },
  {
    id: "yaml",
    name: "YAML File",
    icon: "file-yaml",
    extension: "yaml",
    nameStyle: "asis",
    kinds: [{ id: "yaml", label: "YAML file", body: "$END$\n" }],
  },
  {
    id: "toml",
    name: "TOML File",
    icon: "file-toml",
    extension: "toml",
    nameStyle: "asis",
    kinds: [{ id: "toml", label: "TOML file", body: "$END$\n" }],
  },
  {
    id: "shell",
    name: "Shell Script",
    icon: "file-shell",
    extension: "sh",
    nameStyle: "asis",
    kinds: [
      { id: "bash", label: "Bash script", body: "#!/usr/bin/env bash\nset -euo pipefail\n\n$END$\n" },
      { id: "sh", label: "POSIX sh script", body: "#!/bin/sh\n\n$END$\n" },
      { id: "zsh", label: "Zsh script", body: "#!/usr/bin/env zsh\n\n$END$\n", extension: "zsh" },
    ],
  },
  {
    id: "dockerfile",
    name: "Dockerfile",
    icon: "file-docker",
    extension: "",
    nameStyle: "asis",
    defaultName: "Dockerfile",
    kinds: [{ id: "dockerfile", label: "Dockerfile", body: "FROM $END$\n" }],
  },
  {
    id: "editorconfig",
    name: "EditorConfig File",
    icon: "file-config",
    extension: "",
    nameStyle: "asis",
    defaultName: ".editorconfig",
    kinds: [
      {
        id: "default",
        label: "Default properties",
        body: "root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\nindent_style = space\nindent_size = 4\ninsert_final_newline = true\ntrim_trailing_whitespace = true\n$END$",
      },
      { id: "root", label: "Root only", body: "root = true\n\n[*]\n$END$\n" },
    ],
  },
  {
    id: "gitignore",
    name: ".gitignore File",
    icon: "file-git",
    extension: "",
    nameStyle: "asis",
    defaultName: ".gitignore",
    kinds: [{ id: "gitignore", label: ".gitignore file", body: "$END$\n" }],
  },
  {
    id: "tomcat-context",
    name: "Tomcat Context Descriptor",
    icon: "file-config",
    extension: "xml",
    nameStyle: "asis",
    defaultName: "context",
    kinds: [
      {
        id: "context",
        label: "Tomcat Context Descriptor (context.xml)",
        body: '<?xml version="1.0" encoding="UTF-8"?>\n<Context antiJARLocking="true" path="$END$">\n</Context>\n',
      },
    ],
  },
  {
    id: "unity-script",
    name: "Unity Script",
    icon: "class",
    extension: "cs",
    nameStyle: "pascal",
    packageStyle: "csharp",
    kinds: [
      {
        id: "monobehaviour",
        label: "MonoBehaviour",
        body: 'using UnityEngine;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\npublic class $NAME$ : MonoBehaviour\n{\n    private void Start()\n    {\n        $END$\n    }\n\n    private void Update()\n    {\n    }\n}\n',
      },
      {
        id: "scriptableobject",
        label: "ScriptableObject",
        body: 'using UnityEngine;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\n[CreateAssetMenu(fileName = "$NAME$", menuName = "Scriptable Objects/$NAME$")]\npublic class $NAME$ : ScriptableObject\n{\n    $END$\n}\n',
      },
      {
        id: "editor",
        label: "Custom Editor",
        body: 'using UnityEditor;\nusing UnityEngine;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\n[CustomEditor(typeof($END$))]\npublic class $NAME$ : Editor\n{\n    public override void OnInspectorGUI()\n    {\n        DrawDefaultInspector();\n    }\n}\n',
      },
      {
        id: "editorwindow",
        label: "Editor Window",
        body: 'using UnityEditor;\nusing UnityEngine;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\npublic class $NAME$ : EditorWindow\n{\n    [MenuItem("Window/$NAME$")]\n    public static void ShowWindow()\n    {\n        GetWindow<$NAME$>("$NAME$");\n    }\n\n    private void OnGUI()\n    {\n        $END$\n    }\n}\n',
      },
      {
        id: "editmode-test",
        label: "EditMode Test",
        body: 'using NUnit.Framework;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\npublic class $NAME$\n{\n    [Test]\n    public void SimplePasses()\n    {\n        $END$\n    }\n}\n',
      },
      {
        id: "playmode-test",
        label: "PlayMode Test",
        body: 'using System.Collections;\nusing NUnit.Framework;\nusing UnityEngine.TestTools;\n\nnamespace $PACKAGE$;\n\n#parse("File Header")\npublic class $NAME$\n{\n    [UnityTest]\n    public IEnumerator WithEnumeratorPasses()\n    {\n        $END$\n        yield return null;\n    }\n}\n',
      },
    ],
  },
  {
    id: "unity-shader",
    name: "Unity Shader",
    icon: "file",
    extension: "shader",
    nameStyle: "pascal",
    kinds: [
      {
        id: "urp-unlit",
        label: "URP Unlit Shader",
        body: 'Shader "Custom/$NAME$"\n{\n    Properties\n    {\n        [MainTexture] _BaseMap ("Base Map", 2D) = "white" {}\n        [MainColor] _BaseColor ("Base Color", Color) = (1, 1, 1, 1)\n    }\n\n    SubShader\n    {\n        Tags { "RenderType" = "Opaque" "RenderPipeline" = "UniversalPipeline" }\n\n        Pass\n        {\n            HLSLPROGRAM\n            #pragma vertex vert\n            #pragma fragment frag\n\n            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"\n\n            struct Attributes\n            {\n                float4 positionOS : POSITION;\n                float2 uv : TEXCOORD0;\n            };\n\n            struct Varyings\n            {\n                float4 positionHCS : SV_POSITION;\n                float2 uv : TEXCOORD0;\n            };\n\n            TEXTURE2D(_BaseMap);\n            SAMPLER(sampler_BaseMap);\n\n            CBUFFER_START(UnityPerMaterial)\n                half4 _BaseColor;\n                float4 _BaseMap_ST;\n            CBUFFER_END\n\n            Varyings vert(Attributes IN)\n            {\n                Varyings OUT;\n                OUT.positionHCS = TransformObjectToHClip(IN.positionOS.xyz);\n                OUT.uv = TRANSFORM_TEX(IN.uv, _BaseMap);\n                return OUT;\n            }\n\n            half4 frag(Varyings IN) : SV_Target\n            {\n                $END$return SAMPLE_TEXTURE2D(_BaseMap, sampler_BaseMap, IN.uv) * _BaseColor;\n            }\n            ENDHLSL\n        }\n    }\n}\n',
      },
      {
        id: "builtin-unlit",
        label: "Built-in Unlit Shader",
        body: 'Shader "Unlit/$NAME$"\n{\n    Properties\n    {\n        _MainTex ("Texture", 2D) = "white" {}\n    }\n    SubShader\n    {\n        Tags { "RenderType" = "Opaque" }\n        LOD 100\n\n        Pass\n        {\n            CGPROGRAM\n            #pragma vertex vert\n            #pragma fragment frag\n            #include "UnityCG.cginc"\n\n            struct appdata\n            {\n                float4 vertex : POSITION;\n                float2 uv : TEXCOORD0;\n            };\n\n            struct v2f\n            {\n                float2 uv : TEXCOORD0;\n                float4 vertex : SV_POSITION;\n            };\n\n            sampler2D _MainTex;\n            float4 _MainTex_ST;\n\n            v2f vert(appdata v)\n            {\n                v2f o;\n                o.vertex = UnityObjectToClipPos(v.vertex);\n                o.uv = TRANSFORM_TEX(v.uv, _MainTex);\n                return o;\n            }\n\n            fixed4 frag(v2f i) : SV_Target\n            {\n                $END$return tex2D(_MainTex, i.uv);\n            }\n            ENDCG\n        }\n    }\n}\n',
      },
      {
        id: "compute",
        label: "Compute Shader",
        extension: "compute",
        body: "#pragma kernel CSMain\n\nRWTexture2D<float4> Result;\n\n[numthreads(8, 8, 1)]\nvoid CSMain(uint3 id : SV_DispatchThreadID)\n{\n    $END$Result[id.xy] = float4(id.x & id.y, (id.x & 15) / 15.0, (id.y & 15) / 15.0, 0.0);\n}\n",
      },
      {
        id: "hlsl-include",
        label: "HLSL Include",
        extension: "hlsl",
        body: "#ifndef $NAME$_INCLUDED\n#define $NAME$_INCLUDED\n\n$END$\n\n#endif\n",
      },
    ],
  },
  {
    id: "unity-asmdef",
    name: "Unity Assembly Definition",
    icon: "file-json",
    extension: "asmdef",
    nameStyle: "asis",
    kinds: [
      {
        id: "asmdef",
        label: "Assembly Definition",
        body: '{\n    "name": "$NAME$",\n    "rootNamespace": "$NAME$",\n    "references": [$END$],\n    "includePlatforms": [],\n    "excludePlatforms": [],\n    "allowUnsafeCode": false,\n    "overrideReferences": false,\n    "precompiledReferences": [],\n    "autoReferenced": true,\n    "defineConstraints": [],\n    "versionDefines": [],\n    "noEngineReferences": false\n}\n',
      },
      {
        id: "asmref",
        label: "Assembly Definition Reference",
        extension: "asmref",
        body: '{\n    "reference": "$END$"\n}\n',
      },
    ],
  },
];
