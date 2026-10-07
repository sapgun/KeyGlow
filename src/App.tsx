import { useEffect } from "react";
import { Keyboard } from "./components/Keyboard/Keyboard";
import { LayoutSelector } from "./components/LayoutSelector";
import { Onboarding } from "./components/Onboarding";
import { ProfileSelector } from "./components/ProfileSelector";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { CatLockButton } from "./components/CatLockButton";
import { getLayout, layoutHint, layoutName, profileLabel } from "./lib/layouts";
import * as api from "./lib/tauri";
import { useAppStore } from "./stores/appStore";
import { useLocale, useT } from "./hooks/useT";

export default function App() {
  const store = useAppStore();
  const t = useT();
  const locale = useLocale();
  const layout = getLayout(store.layoutId);
  const profile = store.profiles.find((item) => item.id === store.profileId);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    store.hydrate().catch((err) => {
      useAppStore.setState({ error: String(err), hydrated: true });
    });

    const onWindowDown = (event: KeyboardEvent) => {
      if (event.repeat || !event.code) return;
      useAppStore.getState().notePress(event.code, true);
    };
    const onWindowUp = (event: KeyboardEvent) => {
      if (!event.code) return;
      useAppStore.getState().notePress(event.code, false);
    };
    // HF-04: key-up events can be missed while the window is hidden or the
    // machine sleeps. Reconcile with the native pressed snapshot instead
    // of trusting the (lossy) event stream alone.
    const resync = () => {
      void useAppStore.getState().resyncPressed();
    };
    const onFocus = () => resync();
    const onVisibility = () => {
      if (document.visibilityState === "visible") resync();
    };
    window.addEventListener("keydown", onWindowDown, true);
    window.addEventListener("keyup", onWindowUp, true);
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);

    // HF-09/HF-05: subscribe independently. If any listener fails, the ones
    // that already registered are unlistened again instead of leaking, and
    // the failure is surfaced (previously Promise.all dropped everything
    // silently on a single rejection).
    const subscribe = (async () => {
      const results = await Promise.allSettled([
        api.onKeyDown((code, seq) => useAppStore.getState().notePress(code, true, seq)),
        api.onKeyUp((code, seq) => useAppStore.getState().notePress(code, false, seq)),
        // P3: state-affecting native events go through the ordering gate —
        // a late/duplicate event can never overwrite newer state.
        api.onStateChanged((payload) =>
          useAppStore
            .getState()
            .applyNativeEvent(
              { disabledKeys: payload.keys },
              payload.safetyEpoch,
              payload.runtimeRevision,
            ),
        ),
        api.onEmergencyUnlock((payload) => {
          useAppStore.getState().showEmergency(payload);
          void useAppStore.getState().hydrate();
          void useAppStore.getState().resyncPressed();
        }),
        api.onProfileChanged(() => {
          void useAppStore.getState().hydrate();
        }),
        api.onCatLock((payload) =>
          useAppStore
            .getState()
            .applyNativeEvent(
              { catLock: payload.locked },
              payload.safetyEpoch,
              payload.runtimeRevision,
            ),
        ),
      ]);
      const fns: Array<() => void> = [];
      let failure: unknown = null;
      for (const result of results) {
        if (result.status === "fulfilled") {
          fns.push(result.value);
        } else if (failure === null) {
          failure = result.reason;
        }
      }
      if (cancelled || failure !== null) {
        for (const fn of fns) {
          try {
            fn();
          } catch {
            // Best effort: never let cleanup itself throw.
          }
        }
        if (failure !== null && !cancelled) {
          useAppStore.setState({ eventError: t("eventSubscribeFailed") });
        }
        return;
      }
      unlisteners.push(...fns);
    })();
    void subscribe.catch(() => {
      // allSettled never rejects; this is unreachable by construction, but
      // a rejected promise must never escape silently (HF-05).
    });

    return () => {
      cancelled = true;
      window.removeEventListener("keydown", onWindowDown, true);
      window.removeEventListener("keyup", onWindowUp, true);
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
      useAppStore.getState().clearPressTracking();
      unlisteners.forEach((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!store.hydrated) {
    return (
      <div className="flex h-screen flex-col bg-bg">
        <TitleBar />
        <div className="grid flex-1 place-items-center text-muted">{t("starting")}</div>
      </div>
    );
  }

  if (!store.onboarded) {
    return (
      <div className="flex h-screen flex-col bg-bg">
        <TitleBar />
        <Onboarding onChoose={(id) => store.markOnboarded(id)} />
      </div>
    );
  }

  return (
    <div className="grid h-screen grid-rows-[44px_minmax(0,1fr)_auto] bg-bg">
      <TitleBar />
      <main className="flex min-h-0 flex-col overflow-hidden px-6 pb-2 pt-5">
        <header className="mb-3 flex flex-shrink-0 flex-wrap items-end justify-between gap-4">
          <div>
            <div className="text-[11px] uppercase tracking-[0.16em] text-muted">{t("device")}</div>
            <div className="text-lg font-medium">{t("genericKeyboard")}</div>
          </div>
          <div className="flex flex-wrap items-end gap-6">
            <LayoutSelector value={store.layoutId} onChange={store.chooseLayout} />
            <ProfileSelector
              profiles={store.profiles}
              value={store.profileId}
              onChange={store.chooseProfile}
              onCreate={store.newProfile}
              onDuplicate={store.copyProfile}
              onRename={store.renameCurrent}
              onDelete={store.removeProfile}
            />
          </div>
        </header>

        {store.catLock && (
          <div className="mb-3 flex items-center justify-between rounded-xl border border-orange-400/40 bg-orange-500/15 px-4 py-3 text-sm">
            <span>{t("catBanner")}</span>
            <span className="text-xs text-muted">{t("catBannerHint")}</span>
          </div>
        )}

        {store.emergencyNotice && (
          <div className="mb-4 flex items-center justify-between rounded-xl border border-lime/25 bg-lime/10 px-4 py-3 text-sm">
            <span>{t("emergencyNotice")}</span>
            <button className="text-xs text-muted" onClick={store.clearEmergency}>
              {t("dismiss")}
            </button>
          </div>
        )}

        {!store.hookActive && (
          <div className="mb-4 rounded-xl border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">
            {t("hookInactive")}
            {store.hookError ? ` ${store.hookError}` : ""}
          </div>
        )}

        {(store.error || store.eventError || (!store.persisted && store.persistError)) && (
          <div className="mb-4 flex items-center justify-between gap-4 rounded-xl border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">
            <span>
              {store.eventError
                ? store.eventError
                : store.persistErrorKind === "newer_version"
                  ? t("newerVersionSettings")
                  : store.error
                    ? `${t("settingsSaveFailed")} ${store.error}`
                    : `${t("settingsUnsaved")} ${store.persistError ?? ""}`}
            </span>
            {!store.eventError && (
              <span className="flex flex-shrink-0 gap-2">
                <button
                  className="h-8 rounded-lg border border-danger/40 px-3 text-xs hover:bg-danger/10"
                  onClick={() => void store.retryPersist()}
                >
                  {t("retry")}
                </button>
                <button
                  className="h-8 rounded-lg border border-danger/40 px-3 text-xs hover:bg-danger/10"
                  onClick={() => void store.enableAll()}
                >
                  {t("enableAll")}
                </button>
              </span>
            )}
          </div>
        )}

        <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-border bg-panel p-4">
          <div className="mb-2 flex flex-shrink-0 items-center justify-between gap-3 text-[11px] uppercase tracking-[0.14em] text-muted">
            <span>
              {layoutName(layout.id, locale)}
              {` · ${layoutHint(layout.id, locale)}`} · {t("keysCount", { n: layout.keys.length })}
            </span>
            <PressHint layout={layout} code={store.lastPressed} pressed={store.pressedKeys} />
          </div>
          <div className="min-h-0 flex-1">
            <Keyboard
              layout={layout}
              disabledKeys={store.disabledKeys}
              pressedKeys={store.pressedKeys}
              staleKeys={store.staleKeys}
              catLock={store.catLock}
              onToggle={store.toggleKey}
            />
          </div>
        </section>

        <div className="mt-3 flex flex-shrink-0 flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-5 text-xs text-muted">
            <Legend swatch="enabled" label={t("legendOn")} />
            <Legend swatch="disabled" label={t("legendOff")} />
            <Legend swatch="pressed" label={t("legendPressed")} />
          </div>
          <div className="flex items-center gap-2">
            <CatLockButton locked={store.catLock} onToggle={() => void store.toggleCatLock()} />
            <label className="mr-3 flex items-center gap-2 text-xs text-muted">
              <input
                type="checkbox"
                checked={store.startWithWindows}
                onChange={(event) => store.setAutostart(event.target.checked)}
              />
              {t("startWithWindows")}
            </label>
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-4 text-sm hover:border-lime/30"
              onClick={store.enableAll}
            >
              {t("enableAll")}
            </button>
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-4 text-sm hover:border-lime/30"
              onClick={store.resetCurrent}
            >
              {t("resetProfile")}
            </button>
          </div>
        </div>
      </main>
      <StatusBar
        hookActive={store.hookActive}
        hookError={store.hookError}
        layoutName={layoutName(layout.id, locale)}
        profileName={profile ? profileLabel(profile, locale) : t("profile.default")}
        disabledCount={store.disabledKeys.length}
      />
    </div>
  );
}

function PressHint({
  layout,
  code,
  pressed,
}: {
  layout: ReturnType<typeof getLayout>;
  code: string | null;
  pressed: string[];
}) {
  const t = useT();
  if (pressed.length === 0 && !code) {
    return <span className="normal-case tracking-normal text-muted">{t("pressHintIdle")}</span>;
  }
  const active = pressed[pressed.length - 1] ?? code;
  const key = layout.keys.find((item) => item.code === active);
  const label = key?.label || active || "";
  const holding = pressed.length > 0;
  return (
    <span className={`normal-case tracking-normal ${holding ? "text-lime" : "text-muted"}`}>
      {holding ? t("pressing") : t("lastInput")}: {label || active}
    </span>
  );
}

function Legend({ swatch, label }: { swatch: "enabled" | "disabled" | "pressed"; label: string }) {
  const styles: Record<string, string> = {
    enabled: "bg-lime/80 shadow-[0_0_8px_rgba(182,242,92,0.7)]",
    disabled: "bg-[#2a2d34] opacity-70",
    pressed: "bg-lime shadow-[0_0_10px_rgba(182,242,92,1)]",
  };
  return (
    <span className="inline-flex items-center gap-2">
      <span className={`h-2.5 w-2.5 rounded-full ${styles[swatch]}`} />
      {label}
    </span>
  );
}
