import { describe, expect, it } from "vitest";
import {
  extractHttpPlanVariables,
  httpPlanCompletions,
  isInsideScriptBlock,
} from "./httpPlanLanguage";

const PLAN = `@host = http://127.0.0.1:8080
@userId = 7

### Get user
GET {{host}}/users/{{userId}}
Accept: application/json

> {%
client.test("status", () => {
  client.assert(response.status === 200);
});
%}
`;

describe("httpPlanLanguage", () => {
  it("extracts file variables from a plan", () => {
    expect(extractHttpPlanVariables(PLAN)).toEqual(["host", "userId"]);
  });

  it("detects whether the cursor is inside a script block", () => {
    expect(isInsideScriptBlock(PLAN, 7)).toBe(false);
    expect(isInsideScriptBlock(PLAN, 9)).toBe(true);
    expect(isInsideScriptBlock(PLAN, 12)).toBe(false);
  });

  it("returns variable completions after interpolation braces", () => {
    const completions = httpPlanCompletions(`${PLAN}\nGET {{`, 14, 7);
    expect(completions.map((completion) => completion.label)).toContain("host");
    expect(completions.map((completion) => completion.label)).toContain("userId");
  });

  it("returns script completions inside JavaScript blocks", () => {
    const completions = httpPlanCompletions(PLAN, 9, 8);
    expect(completions.map((completion) => completion.label)).toContain("client.test");
    expect(completions.map((completion) => completion.label)).toContain("response.status");
    expect(completions.map((completion) => completion.label)).toContain("{{userId}}");
  });

  it("returns request snippets outside JavaScript blocks", () => {
    const completions = httpPlanCompletions(PLAN, 4, 1);
    expect(completions.map((completion) => completion.label)).toContain("GET");
    expect(completions.map((completion) => completion.label)).toContain("POST JSON request");
    expect(completions.map((completion) => completion.label)).not.toContain("client.test");
  });
});
