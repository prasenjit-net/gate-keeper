import type * as Monaco from "monaco-editor";

export const HTTP_PLAN_LANGUAGE_ID = "http-plan";

const HTTP_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const COMMON_HEADERS = [
  "Accept",
  "Authorization",
  "Content-Type",
  "User-Agent",
  "X-Request-Id",
];

const CONTENT_TYPES = ["application/json", "application/xml", "text/plain", "text/html"];

const SCRIPT_COMPLETIONS = [
  {
    label: "client.test",
    detail: "Define an assertion test",
    insertText: 'client.test("${1:name}", () => {\n  ${2}\n});',
  },
  {
    label: "client.assert",
    detail: "Assert a condition",
    insertText: 'client.assert(${1:condition}, "${2:message}");',
  },
  {
    label: "client.log",
    detail: "Add a script log entry",
    insertText: "client.log(${1:value});",
  },
  {
    label: "client.variables.get",
    detail: "Read request, global, or file variable",
    insertText: 'client.variables.get("${1:name}")',
  },
  {
    label: "client.variables.set",
    detail: "Set a request variable",
    insertText: 'client.variables.set("${1:name}", ${2:value});',
  },
  {
    label: "client.variables.file.get",
    detail: "Read a file-level @ variable",
    insertText: 'client.variables.file.get("${1:name}")',
  },
  {
    label: "request.url",
    detail: "Resolved request URL",
    insertText: "request.url",
  },
  {
    label: "request.method",
    detail: "Request method",
    insertText: "request.method",
  },
  {
    label: "response.status",
    detail: "HTTP response status",
    insertText: "response.status",
  },
  {
    label: "response.body",
    detail: "Parsed JSON body or text body",
    insertText: "response.body",
  },
  {
    label: "response.bodyText",
    detail: "Raw response body text",
    insertText: "response.bodyText",
  },
  {
    label: "console.log",
    detail: "Add a console log entry",
    insertText: "console.log(${1:value});",
  },
];

export interface CompletionSpec {
  label: string;
  detail: string;
  insertText: string;
  kind: "keyword" | "field" | "variable" | "snippet" | "function";
}

let monacoRegistered = false;

export function registerHttpPlanLanguage(monaco: typeof Monaco) {
  if (monacoRegistered) return;
  monacoRegistered = true;

  if (!monaco.languages.getLanguages().some((language) => language.id === HTTP_PLAN_LANGUAGE_ID)) {
    monaco.languages.register({
      id: HTTP_PLAN_LANGUAGE_ID,
      extensions: [".http"],
      aliases: ["Gate Keeper HTTP", "HTTP Plan"],
    });
  }

  monaco.languages.setMonarchTokensProvider(HTTP_PLAN_LANGUAGE_ID, {
    ignoreCase: true,
    tokenizer: {
      root: [
        [/^###.*$/, "keyword"],
        [/^@[A-Za-z0-9_.-]+\s*=.*$/, "variable"],
        [new RegExp(`\\b(${HTTP_METHODS.join("|")})\\b`), "type.identifier"],
        [/{{[^}]+}}/, "variable.predefined"],
        [/[<>]\s*{%/, "string.escape"],
        [/%}/, "string.escape"],
        [/^#.*$/, "comment"],
        [/^\/\/.*$/, "comment"],
        [/^[A-Za-z0-9-]+(?=:)/, "attribute.name"],
      ],
    },
  });

  monaco.languages.registerCompletionItemProvider(HTTP_PLAN_LANGUAGE_ID, {
    triggerCharacters: [".", "{", "@", '"'],
    provideCompletionItems(model, position) {
      const specs = httpPlanCompletions(model.getValue(), position.lineNumber, position.column);
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      return {
        suggestions: specs.map((spec) => ({
          label: spec.label,
          detail: spec.detail,
          insertText: spec.insertText,
          kind: completionKind(monaco, spec.kind),
          insertTextRules: spec.kind === "snippet"
            ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
            : undefined,
          range,
        })),
      };
    },
  });

  const typescript = (
    monaco.languages as typeof monaco.languages & {
      typescript?: {
        javascriptDefaults?: {
          addExtraLib(source: string, filePath?: string): void;
        };
      };
    }
  ).typescript;
  typescript?.javascriptDefaults?.addExtraLib(
    scriptGlobalsDeclaration(),
    "gatekeeper-http-script.d.ts",
  );
}

export function defineHttpPlanThemes(monaco: typeof Monaco) {
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
}

export function extractHttpPlanVariables(content: string): string[] {
  const variables = new Set<string>();
  for (const line of content.split(/\r?\n/)) {
    const match = line.match(/^@([A-Za-z0-9_.-]+)\s*=/);
    if (match) variables.add(match[1]);
  }
  return [...variables].sort((a, b) => a.localeCompare(b));
}

export function isInsideScriptBlock(content: string, lineNumber: number): boolean {
  const lines = content.split(/\r?\n/);
  let inside = false;
  for (let index = 0; index < Math.min(lineNumber, lines.length); index += 1) {
    const line = lines[index];
    if (/[<>]\s*{%/.test(line)) inside = true;
    if (/%}/.test(line)) inside = false;
  }
  return inside;
}

export function httpPlanCompletions(content: string, lineNumber: number, column: number): CompletionSpec[] {
  const currentLine = content.split(/\r?\n/)[lineNumber - 1] ?? "";
  const prefix = currentLine.slice(0, Math.max(0, column - 1));
  const variables = extractHttpPlanVariables(content);

  if (prefix.endsWith("{{") || /{{[\w.-]*$/.test(prefix)) {
    return variables.map((variable) => ({
      label: variable,
      detail: "File variable",
      insertText: prefix.endsWith("{{") ? `${variable}}}` : variable,
      kind: "variable",
    }));
  }

  if (isInsideScriptBlock(content, lineNumber)) {
    return scriptCompletions(variables);
  }

  const suggestions: CompletionSpec[] = [
    ...HTTP_METHODS.map((method) => ({
      label: method,
      detail: "HTTP method",
      insertText: `${method} \${1:https://example.com}`,
      kind: "snippet" as const,
    })),
    ...COMMON_HEADERS.map((header) => ({
      label: header,
      detail: "HTTP header",
      insertText: `${header}: \${1:value}`,
      kind: "snippet" as const,
    })),
    ...CONTENT_TYPES.map((contentType) => ({
      label: contentType,
      detail: "Content type",
      insertText: contentType,
      kind: "field" as const,
    })),
    {
      label: "response assertion block",
      detail: "Response handler script block",
      insertText:
        '> {%\nclient.test("${1:status}", () => {\n  client.assert(response.status === ${2:200});\n});\n%}',
      kind: "snippet",
    },
    {
      label: "pre-request script block",
      detail: "Pre-request script block",
      insertText: '< {%\nrequest.variables.set("${1:name}", "${2:value}");\n%}',
      kind: "snippet",
    },
    {
      label: "POST JSON request",
      detail: "HTTP request with JSON body",
      insertText:
        'POST ${1:https://example.com}\nContent-Type: application/json\nAccept: application/json\n\n{\n  "${2:key}": "${3:value}"\n}',
      kind: "snippet",
    },
  ];

  return suggestions;
}

function scriptCompletions(variables: string[]): CompletionSpec[] {
  return [
    ...SCRIPT_COMPLETIONS.map((completion) => ({
      ...completion,
      kind: "snippet" as const,
    })),
    ...variables.map((variable) => ({
      label: `{{${variable}}}`,
      detail: "File variable interpolation",
      insertText: `{{${variable}}}`,
      kind: "variable" as const,
    })),
  ];
}

function completionKind(monaco: typeof Monaco, kind: CompletionSpec["kind"]) {
  switch (kind) {
    case "keyword":
      return monaco.languages.CompletionItemKind.Keyword;
    case "field":
      return monaco.languages.CompletionItemKind.Field;
    case "variable":
      return monaco.languages.CompletionItemKind.Variable;
    case "function":
      return monaco.languages.CompletionItemKind.Function;
    case "snippet":
      return monaco.languages.CompletionItemKind.Snippet;
  }
}

function scriptGlobalsDeclaration(): string {
  return `
declare const client: {
  test(name: string, fn: () => void): void;
  assert(condition: boolean, message?: string): void;
  log(...values: unknown[]): void;
  variables: {
    get(name: string): unknown;
    set(name: string, value: unknown): void;
    file: { get(name: string): unknown };
    global: { get(name: string): unknown; set(name: string, value: unknown): void };
  };
};
declare const request: {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: string | null;
  variables: { get(name: string): unknown; set(name: string, value: unknown): void };
};
declare const response: {
  status: number;
  headers: Record<string, string>;
  contentType: string;
  body: unknown;
  bodyText: string;
  durationMs: number;
};
`;
}
