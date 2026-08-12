import { createContext, useContext } from "react";

import { resolveBridge, type NativeBridge } from "./bridge";

export const BridgeContext = createContext<NativeBridge | null>(null);

/**
 * The bridge for the current runtime. Tests provide a fake through the
 * context so no component ever needs a real backend.
 */
export function useBridge(): NativeBridge {
  const bridge = useContext(BridgeContext);
  return bridge ?? defaultBridge();
}

let cached: NativeBridge | null = null;

function defaultBridge(): NativeBridge {
  cached ??= resolveBridge();
  return cached;
}
