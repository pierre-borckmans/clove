import { List, Minus, Plus, Scan, X } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { GoldenCard } from "@/components/golden-card";
import { BraidLayer, braidLayout } from "@/components/map/braid";
import { BulbLayer, bulbLayout } from "@/components/map/bulb";
import { ageOf } from "@/components/map/geometry";
import { MapInspector } from "@/components/map/inspector";
import {
  ClovePiece,
  cloneLabelSize,
  type MapCtx,
  MergedBadge,
  type Selection,
  WorktreeShelf,
} from "@/components/map/nodes";
import { RingsLayer, ringsLayout } from "@/components/map/rings";
import { Button } from "@/components/ui/button";
import { WorktreeList } from "@/components/worktree-list";
import { useMapCamera } from "@/hooks/use-map-camera";
import { useOps } from "@/lib/ops";
import { usePrefs } from "@/lib/prefs";
import { openIn, usePrCheck, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

const VARIANTS = ["rings", "braid", "bulb"] as const;
type Variant = (typeof VARIANTS)[number];
const VARIANT_KEY = "clove.map.variant";
/** Room the worktree shelf takes at the bottom of the viewport. */
const SHELF_INSET = 44;

function storedVariant(): Variant {
  try {
    const value = localStorage.getItem(VARIANT_KEY);
    return VARIANTS.find((v) => v === value) ?? "rings";
  } catch {
    return "rings";
  }
}

/** An image-to-clone map. Placement shows provenance, not inferred Git ancestry. */
export function RepoGraph({
  status,
  clones,
  worktrees,
}: {
  status: RepoStatus;
  clones: CloneInfo[];
  worktrees: OtherWorktree[];
}) {
  const [variant, setVariant] = useState<Variant>(storedVariant);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [panel, setPanel] = useState<"image" | "worktrees" | null>(null);
  const prefs = usePrefs();
  const { data: prs } = usePrs(status.root);
  usePrCheck(status.root);
  const ops = useOps();
  const refreshing = ops.some(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const current = status.golden?.generation;
  const newest = current ?? Math.max(0, ...clones.map((c) => c.golden_generation));

  const picked = selection?.kind === "clone" ? clones.find((c) => c.name === selection.name) : undefined;
  const pickedWt = selection?.kind === "worktree" ? worktrees.find((w) => w.path === selection.path) : undefined;
  const generation = picked?.golden_generation ?? (selection?.kind === "image" ? selection.generation : current);

  const ctx: MapCtx = {
    newest,
    current,
    prOf: (name) => prs?.get(name)?.pr,
    picked: picked?.name,
    pickedWorktree: pickedWt?.path,
    imageGeneration: picked || pickedWt ? undefined : generation,
    // With an image picked, its clones stay lit and the rest dim.
    focusAge: !picked && selection?.kind === "image" ? ageOf(newest, selection.generation) : null,
    hovered,
    setHovered,
    select: (next) => {
      setSelection(next);
      setPanel(null);
    },
    open: (path) => void openIn(path, prefs.editor),
  };

  const layout = useMemo(() => {
    const labelOf = (c: CloneInfo, tag?: string) => cloneLabelSize(c, (n) => prs?.get(n)?.pr, tag);
    if (variant === "braid") return braidLayout(clones, newest, labelOf);
    if (variant === "bulb") return bulbLayout(clones, newest, labelOf);
    return ringsLayout(clones, newest, labelOf);
  }, [variant, clones, newest, prs]);

  const { viewport, camera, dragging, hand, fit, zoomBy, bindings } = useMapCamera(
    layout.width,
    layout.height,
    worktrees.length ? SHELF_INSET : 0,
  );
  // A new variant is a new world: fit it once it has its size.
  const shown = useRef(variant);
  useEffect(() => {
    if (shown.current === variant) return;
    shown.current = variant;
    fit();
  }, [variant, fit]);

  const empty = current === undefined && !clones.length;

  return (
    <section className="repo-map flex min-h-0 flex-1 flex-col" aria-label="Repository map">
      <div className="flex h-12 shrink-0 items-center gap-3 border-b px-5">
        <h2 className="text-[13px] font-medium">Image lineage</h2>
        <span className="text-[12px] text-muted-foreground">
          {clones.length} clones · {new Set(clones.map((c) => c.golden_generation)).size} image generations
        </span>
        <div className="flex-1" />
        <fieldset className="flex items-center rounded-md bg-muted p-0.5" aria-label="Map layout">
          {VARIANTS.map((v) => (
            <Button
              key={v}
              variant="ghost"
              size="sm"
              aria-pressed={variant === v}
              className="h-6 px-2 text-[11.5px] capitalize aria-pressed:bg-card aria-pressed:shadow-sm"
              onClick={() => {
                setVariant(v);
                try {
                  localStorage.setItem(VARIANT_KEY, v);
                } catch {}
              }}
            >
              {v}
            </Button>
          ))}
        </fieldset>
        {worktrees.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-[12px]"
            onClick={() => setPanel(panel === "worktrees" ? null : "worktrees")}
          >
            <List className="size-3.5" /> Other worktrees <span className="text-faint">{worktrees.length}</span>
          </Button>
        )}
      </div>
      <p id="map-navigation-help" className="sr-only">
        Placement shows image origin, not Git ancestry. Drag or scroll to pan. Pinch or hold Command or Control while
        scrolling to zoom. Focus the map and use arrow keys to pan, plus and minus to zoom, or zero to fit. Hold Space
        to drag over a node.
      </p>
      <section
        ref={viewport}
        {...bindings}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: This canvas implements keyboard pan, zoom, and fit.
        tabIndex={0}
        role="application"
        aria-label="Interactive image map"
        aria-describedby="map-navigation-help"
        data-panning={dragging || undefined}
        data-hand={hand || undefined}
        className="map-viewport relative min-h-0 flex-1 overflow-clip outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
        style={{
          backgroundPosition: `${camera.x}px ${camera.y}px`,
          backgroundSize: `${18 * camera.scale}px ${18 * camera.scale}px`,
        }}
      >
        <div
          className="map-world absolute top-0 left-0"
          style={{
            width: layout.width,
            height: layout.height,
            transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.scale})`,
            transformOrigin: "top left",
          }}
        >
          {empty ? (
            <div className="flex h-full items-center justify-center text-[13px] text-muted-foreground">
              Build a golden image to start branching.
            </div>
          ) : layout.kind === "braid" ? (
            <BraidLayer layout={layout} ctx={ctx} />
          ) : layout.kind === "bulb" ? (
            <BulbLayer layout={layout} ctx={ctx} />
          ) : (
            <RingsLayer layout={layout} ctx={ctx} />
          )}
        </div>
        <WorktreeShelf worktrees={worktrees} ctx={ctx} />
      </section>
      <div className="flex h-9 shrink-0 items-center gap-2 border-t px-4 text-[10.5px] text-muted-foreground">
        <LegendItem swatch={<Swatch skin="map-skin-peeled" />}>mounted</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-unmounted" />}>unmounted</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-modified" cut />}>modified</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-bruised" />}>needs attention</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-merged" merged />}>merged</LegendItem>
        {variant === "rings" ? (
          <LegendItem swatch={<span className="w-4 border-t border-dashed border-faint" />}>
            older image, one ring in
          </LegendItem>
        ) : (
          <span className="ml-2 hidden text-faint min-[1240px]:inline">
            {variant === "braid" ? "One head per image, newest on the right" : "Slices: the current image's clones"}
          </span>
        )}
        <span className="ml-2 hidden text-faint min-[1400px]:inline">Drag to pan · pinch or ⌘/Ctrl scroll to zoom</span>
        <div className="flex-1" />
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Zoom out" onClick={() => zoomBy(1 / 1.2)}>
          <Minus className="size-3" />
        </Button>
        <span className="w-9 text-center font-mono">{Math.round(camera.scale * 100)}%</span>
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Zoom in" onClick={() => zoomBy(1.2)}>
          <Plus className="size-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Fit map" onClick={fit}>
          <Scan className="size-3.5" />
        </Button>
      </div>
      <MapInspector
        status={status}
        clones={clones}
        picked={picked}
        pickedWt={pickedWt}
        generation={generation}
        panel={panel}
        setPanel={setPanel}
        refreshing={refreshing}
      />
      {panel && (
        <div className="map-details absolute inset-x-4 bottom-[106px] z-10 max-h-[70%] overflow-y-auto rounded-xl border bg-background p-3 shadow-xl">
          <div className="mb-2 flex items-center justify-between px-1 text-[12px] font-medium">
            <span>{panel === "image" ? "Image details" : "Other worktrees"}</span>
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-6"
              aria-label="Close details"
              onClick={() => setPanel(null)}
            >
              <X className="size-3.5" />
            </Button>
          </div>
          {panel === "image" ? (
            <GoldenCard status={status} />
          ) : (
            <WorktreeList root={status.root} worktrees={worktrees} />
          )}
        </div>
      )}
    </section>
  );
}

function LegendItem({ swatch, children }: { swatch: ReactNode; children: ReactNode }) {
  return (
    <span className="mr-2 flex items-center gap-1.5 whitespace-nowrap">
      {swatch}
      {children}
    </span>
  );
}

function Swatch({ skin, cut, merged }: { skin: string; cut?: boolean; merged?: boolean }) {
  return (
    <span className={cn("map-piece relative flex h-4 w-3 items-center justify-center", merged && "mr-1.5", skin)}>
      <ClovePiece length={15} angle={0} cut={cut} />
      {merged && <MergedBadge className="-top-0.5 -right-1.5 size-2.5 [&_svg]:size-2" />}
    </span>
  );
}
