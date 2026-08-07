import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, waitFor, cleanup, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

/**
 * The banner is the whole point of this update flow: the installer is already
 * on disk by the time it appears, so "install" is one click. Ported alongside
 * AURA's `UpdateBanner.test.ts`, which covers the same behaviours.
 */

// Hoisted so the module mocks below can see them.
const { invokeMock, listenMock, fireUpdateReady } = vi.hoisted(() => {
  let handler: ((e: { payload: unknown }) => void) | null = null;
  return {
    invokeMock: vi.fn(),
    listenMock: vi.fn(async (_event: string, cb: (e: { payload: unknown }) => void) => {
      handler = cb;
      return () => {
        handler = null;
      };
    }),
    fireUpdateReady: (payload: unknown) => handler?.({ payload }),
  };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

import UpdateBanner from './UpdateBanner';

/** Default backend: nothing staged, install succeeds. */
function backend(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd in overrides) {
      const value = overrides[cmd];
      return Promise.resolve(typeof value === 'function' ? (value as () => unknown)() : value);
    }
    if (cmd === 'get_staged_update') return Promise.resolve(null);
    if (cmd === 'install_update') return Promise.resolve({ ok: true });
    if (cmd === 'dismiss_update') return Promise.resolve(null);
    return Promise.resolve(null);
  });
}

beforeEach(() => {
  cleanup();
  invokeMock.mockReset();
  listenMock.mockClear();
  backend();
});

describe('UpdateBanner', () => {
  it('stays out of the way until an update is actually staged', async () => {
    render(<UpdateBanner />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_staged_update'));
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('names the version once the installer is downloaded', async () => {
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    expect(await screen.findByRole('status')).toHaveTextContent('0.3.0');
    expect(screen.getByRole('status')).toHaveTextContent('ready to install');
  });

  it('shows an update that was staged before it mounted', async () => {
    // A reload can miss the event, which would otherwise hide an installer
    // that is sitting on disk ready to go.
    backend({ get_staged_update: { version: '0.4.0', tag: 'v0.4.0' } });
    render(<UpdateBanner />);
    expect(await screen.findByRole('status')).toHaveTextContent('0.4.0');
  });

  it('installs on click', async () => {
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    await screen.findByRole('status');
    await userEvent.click(screen.getByRole('button', { name: /restart & install/i }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('install_update'));
  });

  it('shows why it failed instead of quietly doing nothing', async () => {
    backend({ install_update: { ok: false, error: 'installer is missing' } });
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    await screen.findByRole('status');
    await userEvent.click(screen.getByRole('button', { name: /restart & install/i }));

    expect(await screen.findByText('installer is missing')).toBeTruthy();
    // Still usable: a failed install must not leave the button dead.
    const button = screen.getByRole('button', { name: /restart & install/i });
    expect((button as HTMLButtonElement).disabled).toBe(false);
  });

  it('surfaces a thrown error rather than hanging on "Restarting…"', async () => {
    backend({
      install_update: () => {
        throw new Error('bridge is gone');
      },
    });
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    await screen.findByRole('status');
    await userEvent.click(screen.getByRole('button', { name: /restart & install/i }));

    expect(await screen.findByText(/bridge is gone/)).toBeTruthy();
  });

  it('"Later" hides the banner without skipping the version', async () => {
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    await screen.findByRole('status');
    await userEvent.click(screen.getByRole('button', { name: /^later$/i }));

    expect(invokeMock).toHaveBeenCalledWith('dismiss_update', { tag: 'v0.3.0', skip: false });
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });

  it('"Skip this version" persists the skip', async () => {
    render(<UpdateBanner />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    act(() => fireUpdateReady({ version: '0.3.0', tag: 'v0.3.0' }));
    await screen.findByRole('status');
    await userEvent.click(screen.getByRole('button', { name: /skip this version/i }));

    expect(invokeMock).toHaveBeenCalledWith('dismiss_update', { tag: 'v0.3.0', skip: true });
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });

  it('is silent when there is no Tauri bridge at all', async () => {
    // In a plain browser nothing can stage an installer, so the component must
    // not throw or render a broken bar.
    listenMock.mockRejectedValueOnce(new Error('no bridge'));
    invokeMock.mockRejectedValue(new Error('no bridge'));
    render(<UpdateBanner />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(screen.queryByRole('status')).toBeNull();
  });
});
