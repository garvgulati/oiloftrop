declare module "css-tree/tokenizer" {
  export const tokenize: typeof import("css-tree").tokenize;
  export const tokenTypes: typeof import("css-tree").tokenTypes;
}

declare module "css-tree/utils" {
  export const ident: typeof import("css-tree").ident;
  export const string: typeof import("css-tree").string;
  export const url: typeof import("css-tree").url;
}
