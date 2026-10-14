import { describe, expect, it } from "vitest";
import {
  formatIdle,
  groupProjects,
  normalizeInactiveDays,
  selectionForThreshold,
} from "./devprojects";
import type { CleanupItem } from "./types";

const DAY = 86_400;
const NOW = 1_800_000_000;

function devItem(
  id: string,
  root: string,
  size: number,
  idle: number | null,
  ecosystem = "Node.js",
): CleanupItem {
  return {
    id,
    ruleId: "node-modules",
    label: id,
    category: "buildArtifact",
    tier: "review",
    path: `${root}/${id}`,
    sizeBytes: size,
    regenerates: true,
    lastModifiedSecs: null,
    note: "",
    selectedByDefault: false,
    dev: {
      projectRoot: root,
      projectName: root.split("/").pop() ?? root,
      projectKind: "Node.js",
      ecosystem,
      lastActiveSecs: idle === null ? null : NOW - idle * DAY,
    },
  };
}

describe("groupProjects", () => {
  it("groups by project root, most inactive first, unknown last", () => {
    const projects = groupProjects(
      [
        devItem("a1", "C:/code/a", 100, 5),
        devItem("b1", "C:/code/b", 10, 90),
        devItem("a2", "C:/code/a", 300, 5, "Gradle / Android"),
        devItem("c1", "C:/code/c", 999, null),
      ],
      NOW,
    );
    expect(projects.map((p) => p.name)).toEqual(["b", "a", "c"]);
    const a = projects[1];
    expect(a.totalBytes).toBe(400);
    expect(a.ecosystems).toEqual(["Node.js", "Gradle / Android"]);
    expect(a.items[0].id).toBe("a2"); // largest first
  });

  it("ignores items without project context", () => {
    const plain = { ...devItem("x", "C:/x", 1, 1), dev: null };
    expect(groupProjects([plain], NOW)).toEqual([]);
  });
});

describe("selectionForThreshold", () => {
  it("selects only projects idle at least the threshold, never unknown", () => {
    const projects = groupProjects(
      [
        devItem("old", "C:/old", 1, 45),
        devItem("edge", "C:/edge", 1, 30),
        devItem("new", "C:/new", 1, 3),
        devItem("unk", "C:/unk", 1, null),
      ],
      NOW,
    );
    expect(selectionForThreshold(projects, 30)).toEqual({
      old: true,
      edge: true,
      new: false,
      unk: false,
    });
  });
});

describe("formatting and prefs", () => {
  it("formats idle time", () => {
    expect(formatIdle(null)).toBe("unknown");
    expect(formatIdle(0)).toBe("today");
    expect(formatIdle(1)).toBe("1 day");
    expect(formatIdle(45)).toBe("45 days");
    expect(formatIdle(100)).toBe("3 months");
  });

  it("clamps persisted thresholds", () => {
    expect(normalizeInactiveDays(undefined)).toBe(30);
    expect(normalizeInactiveDays(2)).toBe(7);
    expect(normalizeInactiveDays(9999)).toBe(365);
    expect(normalizeInactiveDays(15)).toBe(15);
  });
});
