import { useEffect, useRef, useState } from "react";
import type { KeyboardLayout } from "../../types/keyboard";
import { KeyboardKey } from "./KeyboardKey";

interface KeyboardProps {
  layout: KeyboardLayout;
  disabledKeys: string[];
  pressedKeys: string[];
  catLock?: boolean;
  onToggle: (code: string) => void;
}

export function Keyboard({ layout, disabledKeys, pressedKeys, catLock, onToggle }: KeyboardProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 1100, h: 420 });

  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const update = () => {
      setBox({
        w: Math.max(1, el.clientWidth),
        h: Math.max(1, el.clientHeight),
      });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [layout.id]);

  const extentW = layout.keys.reduce((max, key) => Math.max(max, key.x + key.w), layout.width || 0);
  const extentH = layout.keys.reduce((max, key) => Math.max(max, key.y + key.h), layout.height || 0);
  const unit = Math.min(box.w / Math.max(extentW, 1), box.h / Math.max(extentH, 1));
  const gap = Math.max(3, Math.min(layout.gap, unit * 0.12));
  const kbWidth = extentW * unit;
  const kbHeight = extentH * unit;

  return (
    <div ref={hostRef} className="flex h-full min-h-0 w-full items-center justify-center">
      <div
        className="relative"
        style={{
          width: kbWidth,
          height: kbHeight,
        }}
      >
        {layout.keys.map((keyDef) => {
          const disabled = Boolean(catLock) || disabledKeys.includes(keyDef.code);
          const pressed = pressedKeys.includes(keyDef.code);
          const state = pressed ? "pressed" : disabled ? "disabled" : "enabled";
          return (
            <KeyboardKey
              key={`${keyDef.code}:${keyDef.x}:${keyDef.y}`}
              keyDef={keyDef}
              state={state}
              unit={unit}
              gap={gap}
              onToggle={onToggle}
            />
          );
        })}
      </div>
    </div>
  );
}
