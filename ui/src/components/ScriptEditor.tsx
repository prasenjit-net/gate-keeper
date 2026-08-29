import Editor from "@monaco-editor/react";
import type * as Monaco from "monaco-editor";
import { useCallback } from "react";
import { useTheme } from "../context/ThemeContext";

interface ScriptEditorProps {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  height?: string;
}

const HTTP_KEYWORDS = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
  "HTTP",
  "HTTPS",
];

export default function ScriptEditor({
  value,
  onChange,
  ariaLabel,
  height = "520px",
}: ScriptEditorProps) {
  const { resolved } = useTheme();

  const beforeMount = useCallback((monaco: typeof Monaco) => {
    if (!monaco.languages.getLanguages().some((language) => language.id === "http-plan")) {
      monaco.languages.register({ id: "http-plan" });
      monaco.languages.setMonarchTokensProvider("http-plan", {
        ignoreCase: true,
        tokenizer: {
          root: [
            [/^###.*$/, "keyword"],
            [/^@[A-Za-z0-9_.-]+\s*=.*$/, "variable"],
            [new RegExp(`\\b(${HTTP_KEYWORDS.join("|")})\\b`), "type.identifier"],
            [/{{[^}]+}}/, "variable.predefined"],
            [/^>.*$/, "string"],
            [/^<.*$/, "string.escape"],
            [/^#.*$/, "comment"],
            [/^\/\/.*$/, "comment"],
            [/^[A-Za-z0-9-]+(?=:)/, "attribute.name"],
          ],
        },
      });
    }

    monaco.editor.defineTheme("gate-keeper-light", {
      base: "vs",
      inherit: true,
      rules: [
        { token: "keyword", foreground: "0d7d59", fontStyle: "bold" },
        { token: "variable", foreground: "3f63e0" },
        { token: "variable.predefined", foreground: "9c7215" },
        { token: "attribute.name", foreground: "5d5a70" },
        { token: "string", foreground: "17825c" },
        { token: "comment", foreground: "8f8ca1", fontStyle: "italic" },
      ],
      colors: {
        "editor.background": "#ffffff",
        "editor.foreground": "#2b2938",
        "editorLineNumber.foreground": "#8f8ca1",
        "editorCursor.foreground": "#0d7d59",
        "editor.selectionBackground": "#d8f5e8",
        "editor.lineHighlightBackground": "#f1f0f3",
      },
    });

    monaco.editor.defineTheme("gate-keeper-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "keyword", foreground: "28e99f", fontStyle: "bold" },
        { token: "variable", foreground: "8ba4ff" },
        { token: "variable.predefined", foreground: "dfb35c" },
        { token: "attribute.name", foreground: "a5a1b8" },
        { token: "string", foreground: "5fce97" },
        { token: "comment", foreground: "757088", fontStyle: "italic" },
      ],
      colors: {
        "editor.background": "#1d1b26",
        "editor.foreground": "#eae8f2",
        "editorLineNumber.foreground": "#757088",
        "editorCursor.foreground": "#28e99f",
        "editor.selectionBackground": "#164836",
        "editor.lineHighlightBackground": "#272432",
      },
    });
  }, []);

  return (
    <div
      className="overflow-hidden rounded-lg border border-line-strong bg-surface"
      style={{ height }}
      aria-label={ariaLabel}
    >
      <Editor
        language="http-plan"
        value={value}
        theme={resolved === "dark" ? "gate-keeper-dark" : "gate-keeper-light"}
        beforeMount={beforeMount}
        onChange={(next) => onChange(next ?? "")}
        options={{
          automaticLayout: true,
          fontFamily:
            'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
          fontSize: 13,
          lineNumbersMinChars: 3,
          minimap: { enabled: false },
          padding: { top: 12, bottom: 12 },
          renderLineHighlight: "line",
          scrollBeyondLastLine: false,
          tabSize: 2,
          wordWrap: "on",
        }}
      />
    </div>
  );
}
