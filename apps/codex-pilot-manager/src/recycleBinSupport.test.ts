import { recycleBinEntries } from "./recycleBinSupport.js";

const first = recycleBinEntries(null);
const second = recycleBinEntries(null);

if (first !== second) {
  throw new Error("missing recycle-bin snapshots must reuse a stable empty entries array");
}
