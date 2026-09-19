let pendingDialogs = 0;

/** Native dialogs are outside the DOM; launch routing must wait for them too. */
export async function withNavigationBlock<T>(task: () => Promise<T>): Promise<T> {
  pendingDialogs++;
  try { return await task(); }
  finally { pendingDialogs--; }
}

export function navigationBlocked(): boolean { return pendingDialogs > 0; }
