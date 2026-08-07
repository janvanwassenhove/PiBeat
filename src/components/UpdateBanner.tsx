import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * A quiet bar that appears only once a new version is already downloaded.
 *
 * Ported from AURA's `UpdateBanner.vue`, and the reasoning carries over: the
 * installer is on disk by the time this shows, so installing costs one click
 * and a few seconds. The alternative — a modal the moment an update exists,
 * which *then* starts downloading — interrupts you first and makes you wait
 * for something you already agreed to.
 *
 * Nothing here runs outside the desktop app: without the Tauri bridge the
 * listener never fires and the component stays silent.
 */

interface StagedUpdate {
  version: string;
  tag: string;
}

const UpdateBanner: React.FC = () => {
  const [ready, setReady] = useState<StagedUpdate | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listen<StagedUpdate>('update-ready', (event) => {
      if (!cancelled) setReady(event.payload);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // No Tauri bridge (browser / test): stay silent.
      });

    // A reload can miss the event that fired before this mounted, which would
    // hide an update that is sitting on disk ready to go.
    invoke<StagedUpdate | null>('get_staged_update')
      .then((staged) => {
        if (!cancelled && staged) setReady(staged);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      try {
        unlisten?.();
      } catch {
        // Unregistering can fail if the event bridge is already gone (window
        // closing, webview reloading). Throwing out of an effect cleanup helps
        // nobody — the listener is going away regardless.
      }
    };
  }, []);

  const install = useCallback(async () => {
    if (installing) return;
    setInstalling(true);
    setError('');
    try {
      const result = await invoke<{ ok: boolean; error?: string }>('install_update');
      // On success PiBeat quits and the installer takes over; only a failure
      // gets this far.
      if (!result?.ok) {
        setError(result?.error || 'The update could not be installed.');
        setInstalling(false);
      }
    } catch (e) {
      setError(String(e));
      setInstalling(false);
    }
  }, [installing]);

  const later = useCallback(() => {
    // Deliberately not `skip: true` — "Later" must not silently mean "never".
    // The banner returns on the next check.
    invoke('dismiss_update', { tag: ready?.tag, skip: false }).catch(() => {});
    setReady(null);
  }, [ready]);

  const skip = useCallback(() => {
    invoke('dismiss_update', { tag: ready?.tag, skip: true }).catch(() => {});
    setReady(null);
  }, [ready]);

  if (!ready) return null;

  return (
    <div className="update-bar" role="status">
      <span className="update-icon" aria-hidden="true">&#9650;</span>
      <span className="update-text">
        PiBeat {ready.version} is ready to install.
      </span>
      <button
        className="update-btn update-btn--go"
        disabled={installing}
        onClick={install}
      >
        {installing ? 'Restarting…' : 'Restart & install'}
      </button>
      <button className="update-btn" disabled={installing} onClick={later}>
        Later
      </button>
      <button
        className="update-btn update-btn--quiet"
        disabled={installing}
        onClick={skip}
        title={`Never offer ${ready.version} again`}
      >
        Skip this version
      </button>
      {error && <span className="update-err">{error}</span>}
    </div>
  );
};

export default UpdateBanner;
