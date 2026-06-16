import { useEffect, useState } from "react";
import {
  getQuietModeStatus,
  listenQuietModeStatus,
} from "../services/dashboard";
import type { QuietModeStatus } from "../types/dashboard";

const inactiveQuietMode: QuietModeStatus = {
  active: false,
  reason: null,
};

export function useQuietModeStatus() {
  const [status, setStatus] = useState<QuietModeStatus>(inactiveQuietMode);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void getQuietModeStatus()
      .then((nextStatus) => {
        if (!disposed) {
          setStatus(nextStatus);
        }
      })
      .catch(() => {
        if (!disposed) {
          setStatus(inactiveQuietMode);
        }
      });

    void listenQuietModeStatus((nextStatus) => {
      if (!disposed) {
        setStatus(nextStatus);
      }
    })
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
          return;
        }
        unlisten = nextUnlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return status;
}
