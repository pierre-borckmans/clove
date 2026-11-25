import { Navigate } from "@tanstack/react-router";
import { FolderPlus } from "lucide-react";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { Glyph } from "@/components/glyph";
import { Shortcut } from "@/components/shortcut";
import { Button } from "@/components/ui/button";
import { useRepos } from "@/lib/queries";
import { setUi } from "@/lib/ui";

export function HomePage() {
  const { data: repos, isLoading, error } = useRepos();
  if (repos?.[0]) return <Navigate to="/repo/$repoId" params={{ repoId: repos[0].id }} replace />;
  return (
    <>
      <PaneHeader>
        <span className="text-[14px] font-semibold">clove</span>
      </PaneHeader>
      <div className="flex flex-1 items-center justify-center pb-16">
        {!isLoading && (
          <Empty
            icon={<Glyph className="!size-7" />}
            title={error ? "Could not read clove's state" : "No repositories yet"}
            action={
              <Button onClick={() => setUi({ addRepo: true })} className="gap-2">
                <FolderPlus className="size-4" />
                Add repository…
                <Shortcut
                  keys="mod+o"
                  className="ml-1 [&_kbd]:border-white/25 [&_kbd]:bg-white/15 [&_kbd]:text-white"
                />
              </Button>
            }
          >
            {error
              ? String(error)
              : "Pick a git checkout. clove builds one golden image of it with dependencies installed, then makes ready worktrees from it in about a second."}
          </Empty>
        )}
      </div>
    </>
  );
}
