import { tokenize, tokenTypes } from "css-tree/tokenizer";
import { ident as cssIdent, string as cssString, url as cssUrl } from "css-tree/utils";
import { resolveAddonAssetPath } from "./addon-sandbox-asset-registry";

export interface SandboxAddonFile {
  name: string;
  content: string;
  isMain?: boolean;
}

const ADDON_STYLE_ATTRIBUTE = "data-wealthfolio-addon-style";

export function isCssFile(path: string) {
  return path.toLowerCase().endsWith(".css");
}

function findAddonStyleElement(path: string) {
  return Array.from(
    document.head.querySelectorAll<HTMLStyleElement>(`style[${ADDON_STYLE_ATTRIBUTE}]`),
  ).find((element) => element.getAttribute(ADDON_STYLE_ATTRIBUTE) === path);
}

export function installAddonStyle(path: string, css: string) {
  let styleElement = findAddonStyleElement(path);
  if (!styleElement) {
    styleElement = document.createElement("style");
    styleElement.setAttribute(ADDON_STYLE_ATTRIBUTE, path);
    document.head.appendChild(styleElement);
  }
  styleElement.textContent = css;
}

interface CssUrlToken {
  end: number;
  start: number;
  value: string;
}

interface PendingQuotedCssUrl {
  start: number;
  value?: string;
}

function findCssUrlTokens(css: string) {
  const tokens: CssUrlToken[] = [];
  let pendingQuotedUrl: PendingQuotedCssUrl | undefined;

  tokenize(css, (type, start, end) => {
    if (pendingQuotedUrl) {
      if (type === tokenTypes.WhiteSpace || type === tokenTypes.Comment) {
        return;
      }
      if (pendingQuotedUrl.value === undefined && type === tokenTypes.String) {
        pendingQuotedUrl.value = cssString.decode(css.slice(start, end));
        return;
      }
      if (pendingQuotedUrl.value !== undefined && type === tokenTypes.RightParenthesis) {
        tokens.push({ end, start: pendingQuotedUrl.start, value: pendingQuotedUrl.value });
        pendingQuotedUrl = undefined;
        return;
      }
      pendingQuotedUrl = undefined;
    }

    if (
      type === tokenTypes.AtKeyword &&
      cssIdent.decode(css.slice(start + 1, end)).toLowerCase() === "import"
    ) {
      throw new Error("Addon CSS @import rules are not supported; bundle or package the CSS file");
    }

    if (type === tokenTypes.Url) {
      tokens.push({
        end,
        start,
        value: cssUrl.decode(css.slice(start, end)),
      });
      return;
    }

    if (
      type === tokenTypes.Function &&
      cssIdent.decode(css.slice(start, end - 1)).toLowerCase() === "url"
    ) {
      pendingQuotedUrl = { start };
    }
  });

  return tokens;
}

export async function resolveAddonCssAssetUrls(
  stylesheetPath: string,
  css: string,
  getAssetUrl: (path: string) => Promise<string>,
) {
  const tokens = findCssUrlTokens(css);
  const replacements = await Promise.all(
    tokens.map(async (token) => {
      const rawUrl = token.value;
      const lowerUrl = rawUrl.toLowerCase();
      if (
        !rawUrl ||
        rawUrl.startsWith("#") ||
        lowerUrl.startsWith("data:") ||
        lowerUrl.startsWith("blob:")
      ) {
        return { ...token, replacement: css.slice(token.start, token.end) };
      }
      if (rawUrl.startsWith("//") || /^[a-z][a-z\d+.-]*:/i.test(rawUrl)) {
        throw new Error(
          `Remote CSS URL '${rawUrl}' in '${stylesheetPath}' is not a packaged addon asset`,
        );
      }

      const queryIndex = rawUrl.indexOf("?");
      const fragmentIndex = rawUrl.indexOf("#");
      const pathEnd = [queryIndex, fragmentIndex]
        .filter((index) => index !== -1)
        .reduce((earliest, index) => Math.min(earliest, index), rawUrl.length);
      const path = rawUrl.slice(0, pathEnd);
      const fragment = fragmentIndex === -1 ? "" : rawUrl.slice(fragmentIndex);
      const resolvedPath = resolveAddonAssetPath(path, stylesheetPath);
      const objectUrl = await getAssetUrl(resolvedPath);
      return { ...token, replacement: `url(${JSON.stringify(`${objectUrl}${fragment}`)})` };
    }),
  );

  let result = "";
  let cursor = 0;
  for (const replacement of replacements) {
    result += css.slice(cursor, replacement.start);
    result += replacement.replacement;
    cursor = replacement.end;
  }
  return result + css.slice(cursor);
}

export async function installAddonCssFiles(
  files: SandboxAddonFile[] = [],
  getAssetUrl?: (path: string) => Promise<string>,
) {
  clearAddonStyles();
  for (const file of files) {
    if (isCssFile(file.name) && file.content.trim()) {
      const css = getAssetUrl
        ? await resolveAddonCssAssetUrls(file.name, file.content, getAssetUrl)
        : file.content;
      installAddonStyle(file.name, css);
    }
  }
}

export function clearAddonStyles() {
  for (const styleElement of document.head.querySelectorAll<HTMLStyleElement>(
    `style[${ADDON_STYLE_ATTRIBUTE}]`,
  )) {
    styleElement.remove();
  }
}

export function createCssModuleSource(css: string) {
  return `
const css = ${JSON.stringify(css)};
export default css;
`;
}
