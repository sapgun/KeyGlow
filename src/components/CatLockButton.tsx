import { useT } from "../hooks/useT";

interface CatLockButtonProps {
  locked: boolean;
  onToggle: () => void;
}

export function CatLockButton({ locked, onToggle }: CatLockButtonProps) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onToggle}
      title={locked ? t("catUnlockHint") : t("catLockHint")}
      aria-pressed={locked}
      aria-label={t("catLock")}
      className={`cat-btn ${locked ? "active" : ""}`}
    >
      <CatGlyph />
      <span className="cat-btn-label">{locked ? t("catLockedLabel") : t("catLabel")}</span>
    </button>
  );
}

function CatGlyph() {
  return (
    <svg viewBox="0 0 64 64" width="40" height="40" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12.5 18.5c0-6.2 2.8-12 4.6-14.2.6-.7 1.6-.4 1.8.5l2.2 9.4c1.6-.6 3.4-1 5.4-1.1l.7-8.6c.1-.9 1.2-1.2 1.8-.5 1.7 2.1 4.4 7.6 4.8 13.6 4.4.8 7.9 3.6 9.8 7.4 2.4-.2 5.3.3 7.3 2.3 2.6 2.6 2.8 6.3 1.7 9.4-.8 2.2-1.4 5.8-1.2 9.3.3 5.4-2.2 10.4-8.6 12.4-5.2 1.6-12.3 1.8-19.2.2C15.2 57 10 51.6 9.2 44.2c-.6-5.6.6-10.3 2.4-13.6-1.3-3.4-1.2-8.4.9-12.1Z"
      />
      <circle cx="24.2" cy="28.5" r="2.1" fill="#0c0d10" />
      <circle cx="33.4" cy="28.2" r="2.1" fill="#0c0d10" />
      <path
        fill="#0c0d10"
        d="M27.4 33.4c.4 2.2 2.6 3.4 4.6 2.6.6-.2.7-1 .2-1.3-1.3-.7-2.6-.8-4-.2-.5.2-.9-.3-.8-1.1Z"
      />
      <path
        fill="currentColor"
        d="M46.5 24.8c3.4-4.6 8.6-6.4 11.2-4.6 1.3.9.6 2.4-1.2 3.2-2.4 1.1-5.6 1.6-8.4 2.4-.8.2-1.6-.4-1.6-1Z"
      />
    </svg>
  );
}
