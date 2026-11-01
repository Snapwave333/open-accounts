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

export interface PlatformKnowledgeProfile {
  id: string;
  name: string;
  version: string;
  confidence_level: 'high' | 'medium' | 'low';
  last_verified: string;
  community_guidelines: {
    title: string;
    official_url: string;
    summary: string;
    key_rules: string[];
  };
  terms_of_service: {
    title: string;
    official_url: string;
    summary: string;
    key_rules: string[];
  };
  bot_and_automation_policy: {
    official_url: string;
    is_automation_permitted: boolean;
    allowed_automation_types: string[];
    prohibited_automation_types: string[];
    requires_official_api: boolean;
    notes: string;
  };
  multiple_account_policy: {
    official_url: string;
    allowed: boolean;
    max_accounts_per_identity?: number;
    allowed_purposes: string[];
    prohibits_ban_evasion: boolean;
    notes: string;
  };
  account_creation_requirements: {
    official_url: string;
    requires_email: boolean;
    allows_email_aliases: boolean;
    requires_phone: boolean;
    minimum_age: number;
    captcha_enforced: boolean;
  };
  rate_limits: Array<{
    action: string;
    limit_per_hour?: number;
    limit_per_day?: number;
    official_doc_url: string;
  }>;
}

export interface EmailIdentity {
  identity_id: string;
  email_address: string;
  provider_type: string;
  assigned_account_id?: string;
  platform?: string;
  is_primary: boolean;
  created_at: string;
}

export interface PreflightStepResult {
  step: number;
  name: string;
  passed: boolean;
  details: string;
}

export interface AccountCreationPreflightReport {
  account_id: string;
  platform: string;
  allocated_email: string;
  username: string;
  step_results: PreflightStepResult[];
  is_ready_for_automation: boolean;
}

export interface PolicyAlert {
  alert_id: string;
  platform_id: string;
  severity: string;
  message: string;
  created_at: string;
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  private isTauriAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

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
      email: 'instagram_01@openaccounts.local',
      status: 'active',
      created_at: '2026-10-15T10:00:00Z',
      last_active: '2 mins ago',
    },
    {
      id: 'acc-2',
      platform: 'reddit',
      username: 'u/AutoDevDesk',
      email: 'reddit_dev@openaccounts.local',
      status: 'active',
      created_at: '2026-10-18T14:30:00Z',
      last_active: '5 mins ago',
    },
    {
      id: 'acc-3',
      platform: 'youtube',
      username: 'StudioContentSync',
      email: 'yt_sync@openaccounts.local',
      status: 'active',
      created_at: '2026-10-20T08:00:00Z',
      last_active: '1 hour ago',
    },
    {
      id: 'acc-4',
      platform: 'discord',
      username: 'CommunityBot#9921',
      email: 'discord_mod@openaccounts.local',
      status: 'idle',
      created_at: '2026-10-22T19:00:00Z',
      last_active: '12 mins ago',
    },
    {
      id: 'acc-5',
      platform: 'x',
      username: '@OpenAccountsCore',
      email: 'x_brand@openaccounts.local',
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

  emailIdentities = signal<EmailIdentity[]>([
    {
      identity_id: 'eid-01',
      email_address: 'instagram_brand_01@openaccounts.io',
      provider_type: 'custom_domain_api',
      platform: 'instagram',
      is_primary: true,
      created_at: '2026-10-28T09:00:00Z',
    },
    {
      identity_id: 'eid-02',
      email_address: 'reddit_eng_02@openaccounts.io',
      provider_type: 'custom_domain_api',
      platform: 'reddit',
      is_primary: true,
      created_at: '2026-10-29T11:30:00Z',
    },
    {
      identity_id: 'eid-03',
      email_address: 'discord_lead_bot@openaccounts.io',
      provider_type: 'microsoft_graph',
      platform: 'discord',
      is_primary: true,
      created_at: '2026-10-30T14:15:00Z',
    },
  ]);

  policyAlerts = signal<PolicyAlert[]>([
    {
      alert_id: 'palert-01',
      platform_id: 'reddit',
      severity: 'Info',
      message: 'Reddit API v2 Data Policy verified current. No action required.',
      created_at: '2026-10-31 20:10:00',
    },
    {
      alert_id: 'palert-02',
      platform_id: 'x',
      severity: 'Warning',
      message: 'X developer tier endpoint rate limit update detected. Auto-throttling active.',
      created_at: '2026-10-31 19:40:00',
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
      case 'list_email_identities':
        return this.emailIdentities() as unknown as T;
      case 'allocate_email_identity': {
        const newIdent: EmailIdentity = {
          identity_id: `eid-${Date.now()}`,
          email_address: `${args?.['prefix'] || 'account'}_${Math.random().toString(36).substring(7)}@openaccounts.io`,
          provider_type: 'custom_domain_api',
          platform: args?.['platform'] as string,
          is_primary: true,
          created_at: new Date().toISOString(),
        };
        this.emailIdentities.update((prev) => [newIdent, ...prev]);
        return newIdent as unknown as T;
      }
      case 'run_policy_monitor_cycle': {
        const alert: PolicyAlert = {
          alert_id: `palert-${Date.now()}`,
          platform_id: 'all_platforms',
          severity: 'Info',
          message: 'Continuous Policy Intelligence Monitor verified all 22 official platform sources.',
          created_at: new Date().toISOString().replace('T', ' ').substring(0, 19),
        };
        this.policyAlerts.update((prev) => [alert, ...prev]);
        return [alert] as unknown as T;
      }
      case 'simulate_policy_change': {
        const alert: PolicyAlert = {
          alert_id: `palert-${Date.now()}`,
          platform_id: (args?.['platform_id'] as string) || 'instagram',
          severity: 'Critical_Pause',
          message: `Policy Change Alert: ${args?.['platform_id']} updated terms. Affected automations safely paused.`,
          created_at: new Date().toISOString().replace('T', ' ').substring(0, 19),
        };
        this.policyAlerts.update((prev) => [alert, ...prev]);
        return undefined as unknown as T;
      }
      case 'execute_10_step_account_preflight': {
        const plat = (args?.['platform'] as string) || 'instagram';
        const report: AccountCreationPreflightReport = {
          account_id: `acc-${Date.now()}`,
          platform: plat,
          allocated_email: `${plat}_${Date.now()}@openaccounts.io`,
          username: (args?.['desired_username'] as string) || `auto_${plat}_user`,
          is_ready_for_automation: true,
          step_results: [
            { step: 1, name: 'Official Platform Policy Verification', passed: true, details: `Verified community guidelines and API terms for ${plat}.` },
            { step: 2, name: 'Account Eligibility & Multi-Account Checks', passed: true, details: `Authorized under ${plat} multiple account rules.` },
            { step: 3, name: 'Authorized Email Identity Allocation', passed: true, details: `Allocated isolated mailbox via custom domain provisioner.` },
            { step: 4, name: 'Secure Credential Preparation', passed: true, details: `Generated 24-character high-entropy password.` },
            { step: 5, name: 'Registration Execution via Approved Provider', passed: true, details: `Dispatched registration form payload to Chrome Relay session.` },
            { step: 6, name: 'Human Intervention Gate Checkpoint', passed: true, details: `User confirmed legal consent & completed Turnstile challenge.` },
            { step: 7, name: 'Email Verification & Ownership Confirmation', passed: true, details: `Extracted verification OTP code and confirmed ownership.` },
            { step: 8, name: 'Secure Credential Vaulting', passed: true, details: `Credentials encrypted in offline vault with Argon2id key derivation.` },
            { step: 9, name: 'Account Registry Creation', passed: true, details: `Account entity registered and isolated in database.` },
            { step: 10, name: 'Final Compliance & Audit Verification', passed: true, details: `Audit log committed with execution hash.` },
          ],
        };
        this.accounts.update((prev) => [
          {
            id: report.account_id,
            platform: plat,
            username: report.username,
            email: report.allocated_email,
            status: 'active',
            created_at: new Date().toISOString(),
            last_active: 'Just now',
          },
          ...prev,
        ]);
        this.stats.update((s) => ({ ...s, total_accounts: s.total_accounts + 1 }));
        return report as unknown as T;
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
