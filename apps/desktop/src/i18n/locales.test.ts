// Guards requirement B5/I: every language is complete and no display text is
// hardcoded. These tests fail the build when a translation is forgotten.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import handbook from "../help/handbook.json";
import { describe, expect, it } from "vitest";
import { CATEGORY_ORDER } from "../schematic/LibraryPanel";
import deDE from "./locales/de-DE.json";
import enUS from "./locales/en-US.json";
import frFR from "./locales/fr-FR.json";
import zhCN from "./locales/zh-CN.json";
import { matchLanguage } from "./index";

type Tree = { [key: string]: string | Tree };

function flatten(tree: Tree, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") out[path] = value;
    else Object.assign(out, flatten(value, path));
  }
  return out;
}

const locales: Record<string, Record<string, string>> = {
  "de-DE": flatten(deDE),
  "en-US": flatten(enUS),
  "fr-FR": flatten(frFR),
  "zh-CN": flatten(zhCN),
};
const reference = locales["en-US"] ?? {};
const placeholders = (text: string) => (text.match(/{{\w+}}/g) ?? []).sort();

function sourceFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(name) && !name.includes(".test.") ? [path] : [];
  });
}
const sources = sourceFiles("src").map((path) => ({ path, text: readFileSync(path, "utf8") }));

describe.each(Object.entries(locales))("locale %s", (_code, messages) => {
  it("has exactly the reference keys", () => {
    expect(Object.keys(messages).sort()).toEqual(Object.keys(reference).sort());
  });

  it("has no empty text", () => {
    for (const [key, value] of Object.entries(messages)) {
      expect(value.trim(), key).not.toBe("");
    }
  });

  it("uses the same placeholders as the reference", () => {
    for (const [key, value] of Object.entries(reference)) {
      expect(placeholders(messages[key] ?? ""), key).toEqual(placeholders(value));
    }
  });
});

describe("translation use", () => {
  it("defines every key used in the source code", () => {
    for (const { path, text } of sources) {
      for (const match of text.matchAll(/\bt\("([\w.]+)"/g)) {
        expect(reference, `${path}: ${match[1]}`).toHaveProperty([match[1] as string]);
      }
    }
  });

  it("defines every dynamically built module and status key", () => {
    for (const id of ["project", "schematic", "pcb", "view3d", "simulation", "ai", "cloud", "team", "update", "extensions", "help"]) {
      expect(reference).toHaveProperty([`modules.${id}`]);
    }
    for (const status of ["ready", "saved", "created", "opened", "recovered"]) {
      expect(reference).toHaveProperty([`status.${status}`]);
    }
  });

  it("translates every error code the backend can send", () => {
    const backend = ["src-tauri/src/error.rs", "src-tauri/src/lib.rs", "src-tauri/src/pcb.rs", "src-tauri/src/update.rs", "src-tauri/src/extensions.rs"]
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");
    const codes = [...backend.matchAll(/CommandError::new\("([\w.]+)"\)|Self::new\("([\w.]+)"\)/g)];
    expect(codes.length).toBeGreaterThan(5);
    for (const match of codes) {
      const code = match[1] ?? match[2];
      expect(reference, code).toHaveProperty([`errors.${code}`]);
    }
  });

  it("translates every ERC rule", () => {
    const rules = readFileSync("../../crates/tronic-erc/src/lib.rs", "utf8");
    const codes = [...rules.matchAll(/"([a-z_]+)",\s*Severity::/g)].map((match) => match[1]);
    expect(codes.length).toBeGreaterThan(15);
    for (const code of codes) expect(reference, code).toHaveProperty([`erc.${code}`]);
  });

  it("translates every DRC rule", () => {
    const rules = readFileSync("../../crates/tronic-drc/src/lib.rs", "utf8");
    const codes = [...rules.matchAll(/"([a-z_]+)",\s*Severity::/g)].map((match) => match[1]);
    expect(codes.length).toBeGreaterThan(12);
    for (const code of codes) expect(reference, code).toHaveProperty([`drc.${code}`]);
  });

  it("translates every library category", () => {
    const source = readFileSync("../../crates/tronic-library/src/builtin.rs", "utf8");
    const block = source.slice(source.indexOf("CATEGORIES"), source.indexOf("];"));
    const names = [...block.matchAll(/"([a-z]+)"/g)].map((match) => match[1]);
    expect(names.length).toBe(18);
    for (const name of names) {
      expect(reference, name).toHaveProperty([`library.categories.${name}`]);
      expect(CATEGORY_ORDER).toContain(name);
    }
    expect(reference).toHaveProperty(["library.categories.custom"]);
  });

  it("translates the name of every generic part", () => {
    const source = readFileSync("../../crates/tronic-library/src/builtin.rs", "utf8");
    const ids = [...source.matchAll(/"((?:device|mcu|cpu|relay|sensor|display):\w+)"/g)].map((m) => m[1] as string);
    expect(ids.length).toBeGreaterThan(15);
    for (const id of ids) {
      expect(reference, id).toHaveProperty([`library.parts.${id.replace(":", "_")}`]);
    }
  });

  it("contains no hardcoded text inside JSX", () => {
    for (const { path, text } of sources) {
      if (!path.endsWith(".tsx")) continue;
      // Text between tags, on one line or on its own line. Only the product
      // name is exempt, because it is the same in every language.
      for (const match of text.matchAll(/>\s*([^<>{}`;()=]*\p{L}[^<>{}`;()=]*?)\s*<\//gu)) {
        expect(match[1], path).toBe("TronicCAD");
      }
    }
  });

  it("uses full-width punctuation in Chinese text", () => {
    for (const [key, value] of Object.entries(locales["zh-CN"] ?? {})) {
      expect(value.replace(/{{\w+}}/g, ""), key).not.toMatch(/[,:;?!]/);
    }
  });
});

describe("matchLanguage", () => {
  it.each([
    ["de-AT", "de-DE"],
    ["fr-CA", "fr-FR"],
    ["zh-Hans-CN", "zh-CN"],
    ["en-GB", "en-US"],
    ["ja-JP", "en-US"],
    [undefined, "en-US"],
  ])("maps %s to %s", (input, expected) => {
    expect(matchLanguage(input)).toBe(expected);
  });
});

describe("handbook", () => {
  const languages = Object.keys(handbook);

  it("exists in all four languages with the same sections", () => {
    expect(languages.sort()).toEqual(["de-DE", "en-US", "fr-FR", "zh-CN"]);
    const reference = (handbook as Record<string, { id: string }[]>)["en-US"]?.map((section) => section.id);
    expect(reference?.length).toBeGreaterThan(10);
    for (const language of languages) {
      const sections = (handbook as Record<string, { id: string; title: string; text: string[] }[]>)[language] ?? [];
      expect(sections.map((section) => section.id), language).toEqual(reference);
      for (const section of sections) {
        expect(section.title.trim(), `${language} ${section.id}`).not.toBe("");
        expect(section.text.length, `${language} ${section.id}`).toBeGreaterThan(0);
      }
    }
  });

  it("warns in every language where a wrong result could cost money", () => {
    for (const language of languages) {
      const sections = (handbook as Record<string, { id: string; text: string[] }[]>)[language] ?? [];
      for (const id of ["schematic", "simulation", "ai"]) {
        const text = sections.find((section) => section.id === id)?.text.join(" ") ?? "";
        expect(text, `${language} ${id}`).toContain("**");
      }
    }
  });
});
