import Editor from "@monaco-editor/react";
import type * as Monaco from "monaco-editor";
import { useCallback } from "react";
import { useTheme } from "../context/ThemeContext";
import {
  defineHttpPlanThemes,
  HTTP_PLAN_LANGUAGE_ID,
  registerHttpPlanLanguage,
} from "../lib/httpPlanLanguage";

interface ScriptEditorProps {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  height?: string;
}

export default function ScriptEditor({
  value,
  onChange,
  ariaLabel,
  height = "520px",
}: ScriptEditorProps) {
  const { resolved } = useTheme();

  const beforeMount = useCallback((monaco: typeof Monaco) => {
    registerHttpPlanLanguage(monaco);
    defineHttpPlanThemes(monaco);
  }, []);

  return (
    <div
      className="overflow-hidden rounded-lg border border-line-strong bg-surface"
      style={{ height }}
      aria-label={ariaLabel}
    >
      <Editor
        language={HTTP_PLAN_LANGUAGE_ID}
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
