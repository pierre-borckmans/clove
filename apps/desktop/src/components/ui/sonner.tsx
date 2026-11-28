import { CircleCheckIcon, InfoIcon, Loader2Icon, OctagonXIcon, TriangleAlertIcon } from "lucide-react";
import { Toaster as Sonner, type ToasterProps } from "sonner";
import { useTheme } from "@/hooks/use-theme";

const Toaster = ({ ...props }: ToasterProps) => {
  const theme = useTheme();
  return (
    <Sonner
      theme={theme}
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4 text-sage" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4 text-amber" />,
        error: <OctagonXIcon className="size-4 text-destructive" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      toastOptions={{
        classNames: {
          toast: "!text-[13px] !gap-2.5 !py-3 !shadow-[0_8px_30px_-6px_rgb(0_0_0/0.18)]",
          description: "!text-muted-foreground !text-[12px] !font-mono !break-all",
          actionButton: "!bg-foreground/[0.07] !text-foreground !font-medium",
        },
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "10px",
          // Op cards ride in toasts too (see OpsPanel).
          "--width": "380px",
        } as React.CSSProperties
      }
      {...props}
    />
  );
};

export { Toaster };
