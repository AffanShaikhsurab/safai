import { type Component, type JSX } from "solid-js";
import type { Category } from "./types";

/**
 * Friendly, vendor-neutral presentation metadata for each backend `Category`.
 * The UI renders whatever the backend returns and looks up display info here,
 * so adding new rules/categories later needs no screen changes (unknown
 * categories fall back to a sensible generic entry). This is the app's
 * "generalization" surface — we describe findings by *what they are*, never by
 * a single vendor's name.
 */
export interface CategoryMeta {
  label: string;
  /** Full explanation, for card layouts and tooltips. Two sentences. */
  description: string;
  /**
   * One short clause, for single-line hairline rows.
   *
   * The `sky` layout renders category rows at 11.5px on a single line, where
   * `description` overflows badly. Not a duplicate of it — a different length
   * target for a different layout.
   */
  blurb: string;
  Icon: Component<{ class?: string }>;
}

type Svg = (p: { class?: string }) => JSX.Element;

const base = (children: JSX.Element): Svg => (p) =>
  (
    <svg
      class={p.class}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );

// Simple geometric glyphs (stroke = currentColor) that read at small sizes.
const PackageIcon = base(
  <>
    <path d="M12 2 3 7v10l9 5 9-5V7z" />
    <path d="M3 7l9 5 9-5" />
    <path d="M12 12v10" />
  </>,
);
const AppIcon = base(
  <>
    <rect x="3" y="4" width="18" height="14" rx="2" />
    <path d="M3 9h18" />
    <path d="M8 21h8" />
  </>,
);
const BuildIcon = base(
  <>
    <path d="M4 7l8-4 8 4-8 4z" />
    <path d="M4 12l8 4 8-4" />
    <path d="M4 17l8 4 8-4" />
  </>,
);
const TempIcon = base(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v4l3 2" />
  </>,
);
const ModelIcon = base(
  <>
    <rect x="7" y="7" width="10" height="10" rx="2" />
    <path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4" />
  </>,
);
const BrowserIcon = base(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18" />
    <path d="M12 3c3 3 3 15 0 18c-3-3-3-15 0-18z" />
  </>,
);
const StarIcon = base(
  <path d="M12 3l2.4 5.4L20 9.3l-4 3.9 1 5.6-5-2.9-5 2.9 1-5.6-4-3.9 5.6-.9z" />,
);

const META: Record<Category, CategoryMeta> = {
  packageCache: {
    label: "Package Manager Caches",
    blurb: "npm, pip, cargo and gradle downloads. Refilled on next install.",
    description:
      "Downloaded packages kept by tools like npm, uv, gradle or bun. Safe to clear — they re-download on the next install.",
    Icon: PackageIcon,
  },
  editorStorage: {
    label: "App & Editor Data",
    blurb: "Editor workspace history and indexes. Some recent history resets.",
    description:
      "Workspace history, indexes and databases your editors and apps keep. Clearing frees space; some recent history may reset.",
    Icon: AppIcon,
  },
  buildArtifact: {
    label: "Build Artifacts",
    blurb: "node_modules, target and build output. A rebuild brings them back.",
    description:
      "Generated output like node_modules, target and build folders. They rebuild from your project, so removing them is reversible.",
    Icon: BuildIcon,
  },
  temp: {
    label: "Temporary Files",
    blurb: "Scratch files apps left behind. Almost always safe.",
    description:
      "Scratch files apps leave behind. Almost always safe to remove.",
    Icon: TempIcon,
  },
  model: {
    label: "Downloaded Models",
    blurb: "Local AI model weights. These do not come back on their own.",
    description:
      "Large AI/ML model files downloaded by local tools. Only remove ones you no longer use — re-downloading can be slow.",
    Icon: ModelIcon,
  },
  browser: {
    label: "Browser & Runtime Data",
    blurb: "Cached test browsers and runtimes. Re-downloaded on demand.",
    description:
      "Cached browser engines and runtimes (e.g. test browsers). Re-installed on demand.",
    Icon: BrowserIcon,
  },
  other: {
    label: "Other Large Items",
    blurb: "Big folders no rule recognised. Worth a look before removing.",
    description:
      "Sizeable folders that don't fit a known category. Review carefully before removing.",
    Icon: StarIcon,
  },
};

const FALLBACK: CategoryMeta = {
  label: "Other Large Items",
  description: "Review these before removing.",
  blurb: "Unrecognised folder. Review before removing.",
  Icon: StarIcon,
};

export function categoryMeta(category: Category): CategoryMeta {
  return META[category] ?? FALLBACK;
}
