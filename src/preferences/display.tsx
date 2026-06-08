import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

export type NumberDisplayMode = "compact" | "full";

const NUMBER_DISPLAY_STORAGE_KEY = "TokenScopeNumberDisplayMode";
const SHOW_CODEX_USAGE_LIMITS_STORAGE_KEY = "TokenScopeShowCodexUsageLimits";

interface DisplayPreferenceContextValue {
  numberDisplayMode: NumberDisplayMode;
  setNumberDisplayMode: (mode: NumberDisplayMode) => void;
  showCodexUsageLimits: boolean;
  setShowCodexUsageLimits: (show: boolean) => void;
}

const DisplayPreferenceContext = createContext<DisplayPreferenceContextValue | null>(null);

function normalizeNumberDisplayMode(value: string | null | undefined): NumberDisplayMode {
  return value === "full" ? "full" : "compact";
}

function readInitialNumberDisplayMode(): NumberDisplayMode {
  if (typeof window === "undefined") {
    return "compact";
  }

  return normalizeNumberDisplayMode(window.localStorage.getItem(NUMBER_DISPLAY_STORAGE_KEY));
}

function readInitialShowCodexUsageLimits(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const value = window.localStorage.getItem(SHOW_CODEX_USAGE_LIMITS_STORAGE_KEY);
  return value === "true";
}

export function DisplayPreferenceProvider({ children }: { children: ReactNode }) {
  const [numberDisplayMode, setNumberDisplayModeState] = useState<NumberDisplayMode>(
    readInitialNumberDisplayMode,
  );
  const [showCodexUsageLimits, setShowCodexUsageLimitsState] = useState(
    readInitialShowCodexUsageLimits,
  );

  useEffect(() => {
    window.localStorage.setItem(NUMBER_DISPLAY_STORAGE_KEY, numberDisplayMode);
  }, [numberDisplayMode]);

  useEffect(() => {
    window.localStorage.setItem(
      SHOW_CODEX_USAGE_LIMITS_STORAGE_KEY,
      String(showCodexUsageLimits),
    );
  }, [showCodexUsageLimits]);

  useEffect(() => {
    function handleStorage(event: StorageEvent) {
      if (event.key === NUMBER_DISPLAY_STORAGE_KEY) {
        setNumberDisplayModeState(normalizeNumberDisplayMode(event.newValue));
      }

      if (event.key === SHOW_CODEX_USAGE_LIMITS_STORAGE_KEY) {
        setShowCodexUsageLimitsState(event.newValue === "true");
      }
    }

    window.addEventListener("storage", handleStorage);
    return () => window.removeEventListener("storage", handleStorage);
  }, []);

  const setNumberDisplayMode = useCallback((mode: NumberDisplayMode) => {
    setNumberDisplayModeState(normalizeNumberDisplayMode(mode));
  }, []);

  const setShowCodexUsageLimits = useCallback((show: boolean) => {
    setShowCodexUsageLimitsState(show);
  }, []);

  const value = useMemo<DisplayPreferenceContextValue>(
    () => ({
      numberDisplayMode,
      setNumberDisplayMode,
      showCodexUsageLimits,
      setShowCodexUsageLimits,
    }),
    [
      numberDisplayMode,
      setNumberDisplayMode,
      showCodexUsageLimits,
      setShowCodexUsageLimits,
    ],
  );

  return (
    <DisplayPreferenceContext.Provider value={value}>
      {children}
    </DisplayPreferenceContext.Provider>
  );
}

export function useDisplayPreference() {
  const context = useContext(DisplayPreferenceContext);
  if (!context) {
    throw new Error("useDisplayPreference must be used inside DisplayPreferenceProvider");
  }

  return context;
}
