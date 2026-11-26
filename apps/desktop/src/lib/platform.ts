// macOS or not. The app ships for macOS and Linux; a browser (dev, mock,
// screenshots) answers for the machine it runs on.

const ua = typeof navigator === "undefined" ? "" : `${navigator.platform} ${navigator.userAgent}`;

export const isMac = /Mac/i.test(ua);

/** The modifier shown in shortcuts: ⌘ on macOS, Ctrl elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl+";

/** What the machine-wide settings and checks are called. */
export const THIS_MACHINE = isMac ? "This Mac" : "This computer";

/** The file manager, as "Reveal in …" names it. */
export const FILE_MANAGER = isMac ? "Finder" : "Files";
