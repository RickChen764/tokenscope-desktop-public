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

interface DisplayPreferenceContextValue {
  numberDisplayMode: NumberDisplayMode;
  setNumberDisplayMode: (mode: NumberDisplayMode) => void;
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

export function DisplayPreferenceProvider({ children }: { children: ReactNode }) {
  const [numberDisplayMode, setNumberDisplayModeState] = useState<NumberDisplayMode>(
    readInitialNumberDisplayMode,
  );

  useEffect(() => {
    window.localStorage.setItem(NUMBER_DISPLAY_STORAGE_KEY, numberDisplayMode);
  }, [numberDisplayMode]);

  const setNumberDisplayMode = useCallback((mode: NumberDisplayMode) => {
    setNumberDisplayModeState(normalizeNumberDisplayMode(mode));
  }, []);

  const value = useMemo<DisplayPreferenceContextValue>(
    () => ({
      numberDisplayMode,
      setNumberDisplayMode,
    }),
    [numberDisplayMode, setNumberDisplayMode],
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
