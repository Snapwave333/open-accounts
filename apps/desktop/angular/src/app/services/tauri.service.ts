import { Injectable, signal } from '@angular/core';

export interface SystemStats {
  total_accounts: number;
  active_automations: number;
  pending_human_tasks: number;
  system_status: string;
  updated_at: string;
}

export interface AccountRecord {
  id: string;
  platform: string;
  username: string;
  email?: string;
  phone?: string;
  status: string;
  created_at: string;
  last_active?: string;
}

export interface HumanTask {
  id: string;
  account_id?: string;
  automation_id?: string;
  challenge_type: string;
  title: string;
  instructions: string;
  page_url?: string;
  status: string;
  created_at: string;
}

export interface SecretMetadata {
  id: string;
  account_id?: string;
  secret_type: string;
  label: string;
  created_at: string;
}

export interface LMProfile {
  id: string;
  name: string;
  provider_type: string;
  model_id: string;
  endpoint?: string;
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  private isTauriAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  // Local state signals
  stats = signal<SystemStats>({
    total_accounts: 24,
    active_automations: 6,
    pending_human_tasks: 2,
    system_status: 'ONLINE',
    updated_at: new Date().toISOString(),
  });

  accounts = signal<AccountRecord[]>([
    {
      id: 'acc-1',
      platform: 'instagram',
      username: 'growth_brand_ai',
      email: 'ops@brand.io',
      status: 'active',
      created_at: '2026-10-15T10:00:00Z',
      last_active: '2 mins ago',
    },
    {
      id: 'acc-2',
      platform: 'reddit',
      username: 'u/AutoDevDesk',
      email: 'dev@brand.io',
      status: 'active',
      created_at: '2026-10-18T14:30:00Z',
      last_active: '5 mins ago',
    },
    {
      id: 'acc-3',
      platform: 'youtube',
      username: 'StudioContentSync',
      email: 'yt@brand.io',
      status: 'active',
      created_at: '2026-10-20T08:00:00Z',
      last_active: '1 hour ago',
    },
    {
      id: 'acc-4',
      platform: 'discord',
      username: 'CommunityBot#9921',
      email: 'bot@brand.io',
      status: 'idle',
      created_at: '2026-10-22T19:00:00Z',
      last_active: '12 mins ago',
    },
    {
      id: 'acc-5',
      platform: 'x',
      username: '@OpenAccountsCore',
      email: 'social@brand.io',
      status: 'active',
      created_at: '2026-10-25T11:20:00Z',
      last_active: 'Just now',
    },
  ]);

  humanTasks = signal<HumanTask[]>([
    {
      id: 'task-101',
      account_id: 'acc-1',
      challenge_type: 'Captcha',
      title: 'Cloudflare Turnstile Challenge',
      instructions: 'Please verify the CAPTCHA in the live browser preview to finalize registration.',
      page_url: 'https://instagram.com/accounts/emailsignup/',
      status: 'Pending',
      created_at: '2026-10-31T18:30:00Z',
    },
    {
      id: 'task-102',
      account_id: 'acc-2',
      challenge_type: 'LegalConsent',
      title: 'Updated Terms of Service Acceptance',
      instructions: 'Reddit requires manual acceptance of new API & Privacy Terms for automation actors.',
      page_url: 'https://reddit.com/policies/user-agreement',
      status: 'Pending',
      created_at: '2026-10-31T19:10:00Z',
    },
  ]);

  vaultUnlocked = signal<boolean>(false);
  killSwitchActive = signal<boolean>(false);

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (this.isTauriAvailable) {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        return await invoke<T>(cmd, args);
      } catch (err) {
        console.warn(`Tauri invoke '${cmd}' failed, using fallback:`, err);
      }
    }
    return this.mockInvoke<T>(cmd, args);
  }

  private async mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    switch (cmd) {
      case 'get_system_stats':
        return this.stats() as unknown as T;
      case 'list_accounts':
        return this.accounts() as unknown as T;
      case 'create_account': {
        const newAcc: AccountRecord = {
          id: `acc-${Date.now()}`,
          platform: (args?.['platform'] as string) || 'custom',
          username: (args?.['username'] as string) || 'new_account',
          email: args?.['email'] as string,
          status: 'active',
          created_at: new Date().toISOString(),
          last_active: 'Just now',
        };
        this.accounts.update((prev) => [newAcc, ...prev]);
        this.stats.update((s) => ({ ...s, total_accounts: s.total_accounts + 1 }));
        return newAcc.id as unknown as T;
      }
      case 'list_human_tasks':
        return this.humanTasks() as unknown as T;
      case 'resolve_human_task': {
        const taskId = args?.['task_id'] as string;
        this.humanTasks.update((tasks) => tasks.filter((t) => t.id !== taskId));
        this.stats.update((s) => ({
          ...s,
          pending_human_tasks: Math.max(0, s.pending_human_tasks - 1),
        }));
        return undefined as unknown as T;
      }
      case 'is_vault_unlocked':
        return this.vaultUnlocked() as unknown as T;
      case 'unlock_vault':
        this.vaultUnlocked.set(true);
        return undefined as unknown as T;
      case 'lock_vault':
        this.vaultUnlocked.set(false);
        return undefined as unknown as T;
      case 'trigger_kill_switch':
        this.killSwitchActive.set(true);
        this.stats.update((s) => ({ ...s, system_status: 'EMERGENCY_HALTED' }));
        return undefined as unknown as T;
      case 'reset_kill_switch':
        this.killSwitchActive.set(false);
        this.stats.update((s) => ({ ...s, system_status: 'ONLINE' }));
        return undefined as unknown as T;
      case 'is_kill_switch_active':
        return this.killSwitchActive() as unknown as T;
      default:
        return {} as unknown as T;
    }
  }

  async resolveTask(id: string) {
    await this.invoke('resolve_human_task', { task_id: id });
  }

  async toggleKillSwitch() {
    if (this.killSwitchActive()) {
      await this.invoke('reset_kill_switch');
    } else {
      await this.invoke('trigger_kill_switch');
    }
  }
}
