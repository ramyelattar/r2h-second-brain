import type { SVGProps } from "react";

export type IconName =
  | "archive"
  | "arrow"
  | "backup"
  | "chat"
  | "check"
  | "chevron"
  | "citation"
  | "dashboard"
  | "database"
  | "file"
  | "folder"
  | "import"
  | "integrity"
  | "log"
  | "model"
  | "plus"
  | "search"
  | "send"
  | "settings"
  | "spark"
  | "workspace";

const paths: Record<IconName, React.ReactNode> = {
  archive: (
    <>
      <path d="M4 7.5h16M6 7.5v11h12v-11M4.5 4h15v3.5h-15z" />
      <path d="M10 11h4" />
    </>
  ),
  arrow: <path d="m8 5 7 7-7 7M15 12H3" />,
  backup: (
    <>
      <path d="M4 7v5h5" />
      <path d="M5.4 15.5A7.5 7.5 0 1 0 6 7.3L4 10" />
    </>
  ),
  chat: <path d="M5 5.5h14v10H9l-4 3v-13ZM9 10h.01M12 10h.01M15 10h.01" />,
  check: <path d="m5 12 4 4L19 6" />,
  chevron: <path d="m9 6 6 6-6 6" />,
  citation: (
    <>
      <path d="M6 8.5h5V14H7.5A3.5 3.5 0 0 1 4 10.5V8a4 4 0 0 1 4-4" />
      <path d="M15 8.5h5V14h-3.5a3.5 3.5 0 0 1-3.5-3.5V8a4 4 0 0 1 4-4" />
    </>
  ),
  dashboard: (
    <>
      <rect x="4" y="4" width="6" height="6" />
      <rect x="14" y="4" width="6" height="6" />
      <rect x="4" y="14" width="6" height="6" />
      <rect x="14" y="14" width="6" height="6" />
    </>
  ),
  database: (
    <>
      <ellipse cx="12" cy="5.5" rx="7" ry="3" />
      <path d="M5 5.5v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6M5 11.5v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" />
    </>
  ),
  file: <path d="M7 3h7l4 4v14H7V3Zm7 0v5h5M10 12h5M10 16h5" />,
  folder: <path d="M3 7h7l2-2h9v14H3V7Z" />,
  import: (
    <>
      <path d="M12 3v12M7 10l5 5 5-5" />
      <path d="M5 17v4h14v-4" />
    </>
  ),
  integrity: (
    <>
      <path d="M12 3 4.5 6v5.5c0 4.6 3.1 7.8 7.5 9.5 4.4-1.7 7.5-4.9 7.5-9.5V6L12 3Z" />
      <path d="m8.5 12 2.2 2.2 4.8-5" />
    </>
  ),
  log: (
    <>
      <path d="M6 4h12v16H6zM9 8h6M9 12h6M9 16h4" />
    </>
  ),
  model: (
    <>
      <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" />
      <path d="m4 7.5 8 4.5 8-4.5M12 12v9" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m15.5 15.5 5 5" />
    </>
  ),
  send: <path d="m4 4 17 8-17 8 3-8-3-8Zm3 8h14" />,
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.4-2.5 1A8 8 0 0 0 14.7 6L14.3 3h-4.6l-.4 3a8 8 0 0 0-1.7 1.1l-2.5-1-2 3.4 2 1.5a7 7 0 0 0 0 2l-2 1.5 2 3.4 2.5-1A8 8 0 0 0 9.3 18l.4 3h4.6l.4-3a8 8 0 0 0 1.7-1.1l2.5 1 2-3.4-2-1.5a7 7 0 0 0 .1-1Z" />
    </>
  ),
  spark: (
    <path d="m12 3 1.4 5.6L19 10l-5.6 1.4L12 17l-1.4-5.6L5 10l5.6-1.4L12 3ZM19 16l.6 2.4L22 19l-2.4.6L19 22l-.6-2.4L16 19l2.4-.6L19 16Z" />
  ),
  workspace: (
    <>
      <path d="M4 5h7v6H4zM13 5h7v14h-7zM4 13h7v6H4z" />
    </>
  ),
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      {paths[name]}
    </svg>
  );
}
