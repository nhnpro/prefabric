using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Prefabric.Unity.Editor.Link
{
    /// <summary>
    /// Minimal JSON for the link's messages: objects become <c>Dictionary&lt;string, object&gt;</c>,
    /// arrays <c>List&lt;object&gt;</c>, numbers <c>double</c>. The package has no dependencies, and
    /// <c>JsonUtility</c> can't read free-form JSON-RPC params.
    /// </summary>
    internal static class Json
    {
        public static object Parse(string text)
        {
            var reader = new Reader(text);
            var value = reader.Value();
            reader.SkipSpace();
            if (!reader.AtEnd)
                throw new FormatException("trailing characters after JSON value");
            return value;
        }

        public static string Write(object value)
        {
            var sb = new StringBuilder();
            WriteValue(sb, value);
            return sb.ToString();
        }

        static void WriteValue(StringBuilder sb, object value)
        {
            switch (value)
            {
                case null:
                    sb.Append("null");
                    break;
                case string s:
                    WriteString(sb, s);
                    break;
                case bool b:
                    sb.Append(b ? "true" : "false");
                    break;
                case IDictionary<string, object> map:
                    sb.Append('{');
                    var first = true;
                    foreach (var pair in map)
                    {
                        if (!first)
                            sb.Append(',');
                        first = false;
                        WriteString(sb, pair.Key);
                        sb.Append(':');
                        WriteValue(sb, pair.Value);
                    }
                    sb.Append('}');
                    break;
                case IEnumerable list:
                    sb.Append('[');
                    var firstItem = true;
                    foreach (var item in list)
                    {
                        if (!firstItem)
                            sb.Append(',');
                        firstItem = false;
                        WriteValue(sb, item);
                    }
                    sb.Append(']');
                    break;
                case float f:
                    sb.Append(f.ToString("R", CultureInfo.InvariantCulture));
                    break;
                case double d:
                    sb.Append(d.ToString("R", CultureInfo.InvariantCulture));
                    break;
                case IFormattable n:
                    sb.Append(n.ToString(null, CultureInfo.InvariantCulture));
                    break;
                default:
                    WriteString(sb, value.ToString());
                    break;
            }
        }

        static void WriteString(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (var c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20)
                            sb.Append("\\u").Append(((int)c).ToString("x4"));
                        else
                            sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }

        sealed class Reader
        {
            /// <summary>Deeper input is refused: the parser recurses, and a stack overflow can't be caught.</summary>
            const int MaxDepth = 64;

            readonly string text;
            int pos;
            int depth;

            public Reader(string text) => this.text = text;

            public bool AtEnd => pos >= text.Length;

            public void SkipSpace()
            {
                while (pos < text.Length && char.IsWhiteSpace(text[pos]))
                    pos++;
            }

            char Next()
            {
                if (pos >= text.Length)
                    throw new FormatException("unexpected end of JSON");
                return text[pos++];
            }

            void Expect(string word)
            {
                if (string.CompareOrdinal(text, pos, word, 0, word.Length) != 0)
                    throw new FormatException($"expected '{word}' at {pos}");
                pos += word.Length;
            }

            public object Value()
            {
                SkipSpace();
                if (AtEnd)
                    throw new FormatException("unexpected end of JSON");
                switch (text[pos])
                {
                    case '{': return Nested(Object);
                    case '[': return Nested(Array);
                    case '"': return String();
                    case 't': Expect("true"); return true;
                    case 'f': Expect("false"); return false;
                    case 'n': Expect("null"); return null;
                    default: return Number();
                }
            }

            object Nested(Func<object> parse)
            {
                if (++depth > MaxDepth)
                    throw new FormatException("JSON nested too deeply");
                try
                {
                    return parse();
                }
                finally
                {
                    depth--;
                }
            }

            Dictionary<string, object> Object()
            {
                var map = new Dictionary<string, object>();
                pos++;
                SkipSpace();
                if (!AtEnd && text[pos] == '}')
                {
                    pos++;
                    return map;
                }
                while (true)
                {
                    SkipSpace();
                    var key = String();
                    SkipSpace();
                    if (Next() != ':')
                        throw new FormatException($"expected ':' at {pos - 1}");
                    map[key] = Value();
                    SkipSpace();
                    var c = Next();
                    if (c == '}')
                        return map;
                    if (c != ',')
                        throw new FormatException($"expected ',' or '}}' at {pos - 1}");
                }
            }

            List<object> Array()
            {
                var list = new List<object>();
                pos++;
                SkipSpace();
                if (!AtEnd && text[pos] == ']')
                {
                    pos++;
                    return list;
                }
                while (true)
                {
                    list.Add(Value());
                    SkipSpace();
                    var c = Next();
                    if (c == ']')
                        return list;
                    if (c != ',')
                        throw new FormatException($"expected ',' or ']' at {pos - 1}");
                }
            }

            string String()
            {
                if (Next() != '"')
                    throw new FormatException($"expected a string at {pos - 1}");
                var sb = new StringBuilder();
                while (true)
                {
                    var c = Next();
                    if (c == '"')
                        return sb.ToString();
                    if (c != '\\')
                    {
                        sb.Append(c);
                        continue;
                    }
                    var e = Next();
                    switch (e)
                    {
                        case '"': sb.Append('"'); break;
                        case '\\': sb.Append('\\'); break;
                        case '/': sb.Append('/'); break;
                        case 'b': sb.Append('\b'); break;
                        case 'f': sb.Append('\f'); break;
                        case 'n': sb.Append('\n'); break;
                        case 'r': sb.Append('\r'); break;
                        case 't': sb.Append('\t'); break;
                        case 'u':
                            if (pos + 4 > text.Length)
                                throw new FormatException("bad \\u escape");
                            sb.Append((char)Convert.ToInt32(text.Substring(pos, 4), 16));
                            pos += 4;
                            break;
                        default:
                            throw new FormatException($"bad escape '\\{e}'");
                    }
                }
            }

            double Number()
            {
                var start = pos;
                while (pos < text.Length && "+-0123456789.eE".IndexOf(text[pos]) >= 0)
                    pos++;
                if (start == pos)
                    throw new FormatException($"unexpected '{text[pos]}' at {pos}");
                return double.Parse(text.Substring(start, pos - start), NumberStyles.Float, CultureInfo.InvariantCulture);
            }
        }
    }
}
