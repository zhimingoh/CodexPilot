import type { RecycleBinSnapshot } from "./types.js";

const EMPTY_RECYCLE_BIN_ENTRIES: RecycleBinSnapshot["entries"] = [];

export function recycleBinEntries(snapshot: RecycleBinSnapshot | null) {
  return snapshot?.entries ?? EMPTY_RECYCLE_BIN_ENTRIES;
}
