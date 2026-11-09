import { describe, expect, it } from "vitest";
import {
  aggregateDrives,
  driveRoot,
  filterByDrive,
  isOnDrive,
  mountOfPath,
  normalizeMount,
  perDriveTotals,
  reconcileScope,
} from "./drives";
import type { DriveEntry } from "./types";

function drive(
  mount: string,
  overrides: Partial<DriveEntry> = {},
): DriveEntry {
  return {
    mount,
    root: driveRoot(mount),
    label: "",
    fileSystem: "NTFS",
    freeBytes: 100,
    totalBytes: 200,
    usedPercent: 50,
    system: false,
    readable: true,
    kind: "fixed",
    ...overrides,
  };
}

describe("mountOfPath", () => {
  it("extracts and uppercases a Windows drive", () => {
    expect(mountOfPath("C:/Users/a")).toBe("C:");
    expect(mountOfPath("d:\\games\\x")).toBe("D:");
    expect(mountOfPath("c:")).toBe("C:");
  });

  it("returns null for relative and UNC paths", () => {
    expect(mountOfPath("/home/x")).toBeNull();
    expect(mountOfPath("\\\\server\\share\\x")).toBeNull();
    expect(mountOfPath("")).toBeNull();
  });
});

describe("normalizeMount", () => {
  it("accepts the forms a UI or config can produce", () => {
    expect(normalizeMount("c:")).toBe("C:");
    expect(normalizeMount("C:/")).toBe("C:");
    expect(normalizeMount("C:\\")).toBe("C:");
  });

  it("rejects anything that is not one drive letter", () => {
    expect(normalizeMount("CC:")).toBeNull();
    expect(normalizeMount("1:")).toBeNull();
    expect(normalizeMount("C:\\Users")).toBeNull();
  });
});

describe("per-drive totals and filtering", () => {
  const items = [
    { path: "C:/a", sizeBytes: 10 },
    { path: "C:/b", sizeBytes: 5 },
    { path: "D:/c", sizeBytes: 40 },
    { path: "/unix/d", sizeBytes: 2 },
  ];

  it("partitions items by mount, largest first", () => {
    const totals = perDriveTotals(items);
    expect(totals.map((t) => [t.mount, t.bytes, t.count])).toEqual([
      ["D:", 40, 1],
      ["C:", 15, 2],
      [null, 2, 1],
    ]);
  });

  it("filters by mount and treats null as all", () => {
    expect(filterByDrive(items, null)).toHaveLength(4);
    expect(filterByDrive(items, "D:").map((i) => i.path)).toEqual(["D:/c"]);
    expect(isOnDrive("C:/x", "c:")).toBe(true);
    expect(isOnDrive("D:/x", "C:")).toBe(false);
  });
});

describe("aggregateDrives", () => {
  it("sums readable drives and ignores unreadable ones", () => {
    const result = aggregateDrives([
      drive("C:", { freeBytes: 25, totalBytes: 100, usedPercent: 75 }),
      drive("D:", { freeBytes: 75, totalBytes: 100, usedPercent: 25 }),
      drive("E:", { readable: false, freeBytes: 0, totalBytes: 0 }),
    ]);
    expect(result.count).toBe(2);
    expect(result.freeBytes).toBe(100);
    expect(result.totalBytes).toBe(200);
    expect(result.usedPercent).toBe(50);
  });
});

describe("reconcileScope", () => {
  const drives = [
    drive("C:", { system: true }),
    drive("D:"),
    drive("E:", { kind: "network" }),
  ];

  it("selects every readable fixed drive on a first run", () => {
    const result = reconcileScope(drives, null);
    expect(result.selectedDrives).toEqual(["C:", "D:"]);
    expect(result.allDrives).toBe(true);
    expect(result.added).toEqual([]);
  });

  it("keeps an explicit selection and reports missing drives", () => {
    const result = reconcileScope(drives, {
      drives: ["C:", "F:"],
      allDrives: false,
      folders: ["C:/code/app"],
    });
    expect(result.selectedDrives).toEqual(["C:"]);
    expect(result.missing).toEqual(["F:"]);
    expect(result.folders).toEqual(["C:/code/app"]);
  });

  it("falls back to the system drive when every saved drive is gone", () => {
    const result = reconcileScope(drives, {
      drives: ["F:"],
      allDrives: false,
      folders: [],
    });
    expect(result.selectedDrives).toEqual(["C:"]);
    expect(result.allDrives).toBe(false);
  });

  it("auto-includes a newly appeared drive for an all-drives scope", () => {
    const result = reconcileScope(drives, {
      drives: ["C:"],
      allDrives: true,
      folders: [],
    });
    expect(result.selectedDrives).toEqual(["C:", "D:"]);
    expect(result.added).toEqual(["D:"]);
  });

  it("never includes network or unreadable drives in all-drives", () => {
    const result = reconcileScope(
      [...drives, drive("G:", { readable: false })],
      null,
    );
    expect(result.selectedDrives).toEqual(["C:", "D:"]);
  });
});
