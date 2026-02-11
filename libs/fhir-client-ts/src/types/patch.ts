export interface JsonPatchAdd {
  op: "add";
  path: string;
  value: unknown;
}

export interface JsonPatchRemove {
  op: "remove";
  path: string;
}

export interface JsonPatchReplace {
  op: "replace";
  path: string;
  value: unknown;
}

export interface JsonPatchMove {
  op: "move";
  from: string;
  path: string;
}

export interface JsonPatchCopy {
  op: "copy";
  from: string;
  path: string;
}

export interface JsonPatchTest {
  op: "test";
  path: string;
  value: unknown;
}

export type JsonPatchOperation =
  | JsonPatchAdd
  | JsonPatchRemove
  | JsonPatchReplace
  | JsonPatchMove
  | JsonPatchCopy
  | JsonPatchTest;
