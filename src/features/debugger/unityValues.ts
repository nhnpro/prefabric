/**
 * Unity-aware debugger variable visualizers for Mono Soft Debugger and .NET (netcoredbg) values:
 * - Compact formatting for Vector2/3/4, Vector2Int/3Int, Unity.Mathematics vectors, Rect, Bounds.
 * - Quaternion with computed Euler angles in degrees (Unity YXZ convention).
 * - Color and Color32 with hex readout and inline RGBA color swatch.
 * - LayerMask bit decoding (`Nothing`, `Everything`, or active bit indices).
 * - Clean labels for GameObject, Transform, Components, ScriptableObject, NativeArray<T>,
 *   SerializedProperty, and Unity destroyed-object pseudo-nulls.
 */

export interface UnityShownValue {
  label: string | null;
  text: string;
  /** CSS `rgba(...)` color for an inline swatch chip (`Color` / `Color32`). */
  swatch?: string;
}

const UNITY_NS =
  /^(?:UnityEngine(?:\.[A-Za-z_]\w*)*|UnityEditor(?:\.[A-Za-z_]\w*)*|Unity\.(?:Mathematics|Collections)(?:\.[A-Za-z_]\w*)*)\.(.+)$/;

const NUM = "[-+]?(?:\\d+\\.?\\d*|\\.\\d+)(?:[eE][-+]?\\d+)?";

function trimNum(raw: string, decimals = 3): string {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  const fixed = Number(n.toFixed(decimals));
  return Object.is(fixed, -0) ? "0" : String(fixed);
}

function clampByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function hexByte(n: number): string {
  return clampByte(n).toString(16).padStart(2, "0").toUpperCase();
}

/** Converts a Unity Quaternion `(x, y, z, w)` to Unity Euler angles `(x, y, z)` in degrees. */
export function quaternionToEuler(x: number, y: number, z: number, w: number): [number, number, number] {
  const len = Math.hypot(x, y, z, w);
  if (len < 1e-8) return [0, 0, 0];
  const nx = x / len;
  const ny = y / len;
  const nz = z / len;
  const nw = w / len;

  const RAD2DEG = 180 / Math.PI;
  const sinPitch = 2 * (nw * nx - ny * nz);
  let pitch: number;
  let yaw: number;
  let roll: number;

  if (Math.abs(sinPitch) >= 0.99999) {
    pitch = Math.sign(sinPitch) * 90;
    const m20 = 2 * (nx * nz - nw * ny);
    const m00 = 1 - 2 * (ny * ny + nz * nz);
    yaw = Math.atan2(-m20, m00) * RAD2DEG;
    roll = 0;
  } else {
    pitch = Math.asin(sinPitch) * RAD2DEG;
    const m02 = 2 * (nx * nz + nw * ny);
    const m22 = 1 - 2 * (nx * nx + ny * ny);
    yaw = Math.atan2(m02, m22) * RAD2DEG;
    const m10 = 2 * (nx * ny + nw * nz);
    const m11 = 1 - 2 * (nx * nx + nz * nz);
    roll = Math.atan2(m10, m11) * RAD2DEG;
  }

  const clean = (deg: number) => {
    const r = Math.round(deg * 10) / 10;
    return Object.is(r, -0) ? 0 : r;
  };
  return [clean(pitch), clean(yaw), clean(roll)];
}

/** Shortens a Unity namespace-qualified type name (e.g. `UnityEngine.Vector3` → `Vector3`). */
export function shortUnityType(typeName: string): string | null {
  const m = UNITY_NS.exec(typeName.trim());
  return m ? m[1]! : null;
}

const VECTOR_TYPES = new Set([
  "Vector2",
  "Vector3",
  "Vector4",
  "Vector2Int",
  "Vector3Int",
  "float2",
  "float3",
  "float4",
  "int2",
  "int3",
  "int4",
  "half2",
  "half3",
  "half4",
]);

const TUPLE_RE = new RegExp(`^\\(\\s*(${NUM}(?:\\s*,\\s*${NUM})*)\\s*\\)$`);
const RGBA_RE = new RegExp(`^RGBA\\(\\s*(${NUM})\\s*,\\s*(${NUM})\\s*,\\s*(${NUM})\\s*,\\s*(${NUM})\\s*\\)$`, "i");
const RECT_RE = new RegExp(
  `^\\(\\s*x\\s*:\\s*(${NUM})\\s*,\\s*y\\s*:\\s*(${NUM})\\s*,\\s*width\\s*:\\s*(${NUM})\\s*,\\s*height\\s*:\\s*(${NUM})\\s*\\)$`,
  "i",
);
const BOUNDS_RE = new RegExp(
  `^Center\\s*:\\s*\\(\\s*(${NUM})\\s*,\\s*(${NUM})\\s*,\\s*(${NUM})\\s*\\)\\s*,\\s*Extents\\s*:\\s*\\(\\s*(${NUM})\\s*,\\s*(${NUM})\\s*,\\s*(${NUM})\\s*\\)$`,
  "i",
);
const LAYER_MASK_RE = /^(?:\{\s*(?:m_Bits|value)\s*[:=]\s*)?(-?\d+)\s*\}?$/i;

function formatColor(r: number, g: number, b: number, a: number, isByte: boolean): { text: string; swatch: string } {
  const rb = isByte ? clampByte(r) : clampByte(r * 255);
  const gb = isByte ? clampByte(g) : clampByte(g * 255);
  const bb = isByte ? clampByte(b) : clampByte(b * 255);
  const ab = isByte ? clampByte(a) : clampByte(a * 255);
  const alphaFloat = isByte ? ab / 255 : Math.max(0, Math.min(1, a));
  const hex =
    ab < 255
      ? `#${hexByte(rb)}${hexByte(gb)}${hexByte(bb)}${hexByte(ab)}`
      : `#${hexByte(rb)}${hexByte(gb)}${hexByte(bb)}`;
  const comps = isByte
    ? `(${rb}, ${gb}, ${bb}, ${ab})`
    : `(${trimNum(String(r), 2)}, ${trimNum(String(g), 2)}, ${trimNum(String(b), 2)}, ${trimNum(String(a), 2)})`;
  return {
    text: `${hex} ${comps}`,
    swatch: `rgba(${rb}, ${gb}, ${bb}, ${Number(alphaFloat.toFixed(3))})`,
  };
}

function formatLayerMask(raw: number): string {
  const mask = raw | 0;
  if (mask === 0) return "0 (Nothing)";
  if (mask === -1) return "-1 (Everything)";
  const bits: number[] = [];
  for (let i = 0; i < 32; i++) {
    if ((mask & (1 << i)) !== 0) bits.push(i);
  }
  const suffix = bits.length <= 4 ? `bits ${bits.join(", ")}` : `${bits.length} layers`;
  return `${mask} (${suffix})`;
}

function shown(label: string | null, text: string, swatch?: string): UnityShownValue {
  return swatch ? { label, text, swatch } : { label, text };
}

/**
 * Formats a Unity debugger value when `typeName` or `value` identifies a Unity type.
 * Returns `null` for non-Unity values so standard formatting applies unchanged.
 */
export function unityShownValue(value: string, typeName: string | null | undefined): UnityShownValue | null {
  const v = value.trim();
  const rawType = typeName?.trim() ?? "";
  const short = rawType ? shortUnityType(rawType) : null;

  // Handle Unity's `Name (UnityEngine.Type)` ToString format even for user MonoBehaviour subclasses.
  const unityObjSuffix = /^(.*?)\s*\(((?:UnityEngine|UnityEditor)\.[A-Za-z0-9_.+]+)\)$/.exec(v);
  if (unityObjSuffix) {
    const objName = unityObjSuffix[1]!.trim();
    const fallbackType = shortUnityType(unityObjSuffix[2]!) ?? unityObjSuffix[2]!;
    const labelType = short ?? (rawType ? rawType.split(".").pop()! : fallbackType);
    const display = objName ? `"${objName.replace(/^"(.*)"$/, "$1")}"` : "(unnamed)";
    return shown(`{${labelType}}`, display);
  }

  if (!short) return null;
  const label = `{${short}}`;

  // Null or Unity destroyed-object pseudo-null (`"null"`).
  if (v === "null" || v === "None") {
    return shown(label, "null");
  }
  if (v === '"null"' || v === "<destroyed>") {
    return shown(label, "null (destroyed)");
  }

  // Vectors (Vector2/3/4, Vector2Int/3Int, Unity.Mathematics float/int vectors).
  if (VECTOR_TYPES.has(short)) {
    const m = TUPLE_RE.exec(v);
    if (m) {
      const nums = m[1]!.split(",").map((part) => trimNum(part.trim()));
      return shown(label, `(${nums.join(", ")})`);
    }
    return shown(label, v);
  }

  // Quaternion with Euler degree readout.
  if (short === "Quaternion") {
    const m = TUPLE_RE.exec(v);
    if (m) {
      const parts = m[1]!.split(",").map((p) => Number(p.trim()));
      if (parts.length === 4 && parts.every(Number.isFinite)) {
        const [ex, ey, ez] = quaternionToEuler(parts[0]!, parts[1]!, parts[2]!, parts[3]!);
        const q = parts.map((n) => trimNum(String(n), 3)).join(", ");
        return shown(label, `(${q})  euler(${ex}°, ${ey}°, ${ez}°)`);
      }
    }
    return shown(label, v);
  }

  // Color & Color32 with hex readout and color swatch.
  if (short === "Color" || short === "Color32") {
    const m = RGBA_RE.exec(v);
    if (m) {
      const { text, swatch } = formatColor(Number(m[1]), Number(m[2]), Number(m[3]), Number(m[4]), short === "Color32");
      return shown(label, text, swatch);
    }
    return shown(label, v);
  }

  // Rect / RectInt.
  if (short === "Rect" || short === "RectInt") {
    const m = RECT_RE.exec(v);
    if (m) {
      return shown(label, `(${trimNum(m[1]!)}, ${trimNum(m[2]!)}, ${trimNum(m[3]!)} × ${trimNum(m[4]!)})`);
    }
    return shown(label, v);
  }

  // Bounds / BoundsInt.
  if (short === "Bounds" || short === "BoundsInt") {
    const m = BOUNDS_RE.exec(v);
    if (m) {
      const cx = trimNum(m[1]!);
      const cy = trimNum(m[2]!);
      const cz = trimNum(m[3]!);
      const sx = trimNum(String(Number(m[4]) * 2));
      const sy = trimNum(String(Number(m[5]) * 2));
      const sz = trimNum(String(Number(m[6]) * 2));
      return shown(label, `center (${cx}, ${cy}, ${cz}), size (${sx}, ${sy}, ${sz})`);
    }
    return shown(label, v);
  }

  // LayerMask.
  if (short === "LayerMask") {
    const m = LAYER_MASK_RE.exec(v);
    if (m) {
      return shown(label, formatLayerMask(Number(m[1])));
    }
    return shown(label, v);
  }

  // Default for other UnityEngine/UnityEditor/Unity.Collections types: clean short type label.
  return shown(label, v === rawType ? "" : v);
}
