import { useEffect, useRef, useState } from "react";

export type ToastNoticeKind = "error" | "success" | "warning" | "info";

export interface ToastNoticeValue {
  kind: ToastNoticeKind;
  message: string;
}

interface ToastNoticeProps {
  notice: ToastNoticeValue | null;
  onClose: () => void;
}

const TOAST_AUTO_DISMISS_MS = 5000;
const TOAST_EXIT_MS = 180;

export function ToastNotice({ notice, onClose }: ToastNoticeProps) {
  const [isExiting, setIsExiting] = useState(false);
  const [canDismiss, setCanDismiss] = useState(false);
  const hoverRef = useRef(false);
  const onCloseRef = useRef(onClose);
  const exitTimerRef = useRef<number | null>(null);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (!notice) {
      return undefined;
    }

    setIsExiting(false);
    setCanDismiss(false);
    hoverRef.current = false;

    const dismissTimer = window.setTimeout(() => {
      setCanDismiss(true);
      if (!hoverRef.current) {
        setIsExiting(true);
        exitTimerRef.current = window.setTimeout(() => onCloseRef.current(), TOAST_EXIT_MS);
      }
    }, TOAST_AUTO_DISMISS_MS);

    return () => {
      window.clearTimeout(dismissTimer);
      if (exitTimerRef.current !== null) {
        window.clearTimeout(exitTimerRef.current);
        exitTimerRef.current = null;
      }
    };
  }, [notice]);

  if (!notice) {
    return null;
  }

  function holdToast() {
    hoverRef.current = true;
  }

  function releaseToast() {
    hoverRef.current = false;
    if (canDismiss) {
      setIsExiting(true);
      exitTimerRef.current = window.setTimeout(() => onCloseRef.current(), TOAST_EXIT_MS);
    }
  }

  return (
    <div className="toast-viewport" aria-live={notice.kind === "error" ? "assertive" : "polite"}>
      <div
        className={`toast-notice ${notice.kind}${isExiting ? " exiting" : ""}`}
        onMouseEnter={holdToast}
        onMouseLeave={releaseToast}
        onPointerEnter={holdToast}
        onPointerLeave={releaseToast}
        role={notice.kind === "error" || notice.kind === "warning" ? "alert" : "status"}
      >
        <span aria-hidden="true" />
        <p>{notice.message}</p>
      </div>
    </div>
  );
}
