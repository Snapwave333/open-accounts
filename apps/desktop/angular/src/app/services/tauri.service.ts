import { Injectable, signal } from '@angular/core';

export interface SystemStats {
  total_accounts: number;
  active_automations: number;
  pending_human_tasks: number;
  system_status: string;
  updated_at: string;
}

export interface SystemHardwareMetrics {
  cpu_percent: number;
  gpu_percent: number;
  memory_used_gb: number;
  memory_total_gb: number;
  disk_percent: number;
}

export interface ActiveOperation {
  id: string;
  name: string;
  description: string;
  progress: number;
  status: 'running' | 'paused' | 'awaiting_approval' | 'completed';
  platform: string;
  started_at: string;
  steps: string[];
  current_step_index: number;
  logs: string[];
}

export interface NotificationItem {
  id: string;
  title: string;
  message: string;
  timestamp: string;
  type: 'security' | 'workflow' | 'policy' | 'system';
  read: boolean;
}

export interface AccountRecord {
  id: string;
  platform: string;
  username: string;
  email?: string;
  phone?: string;
  status: string;
  health_score?: number;
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

export interface BrandProfile {
  brand_id: string;
  name: string;
  tagline: string;
  visual_identity: {
    primary_hex: string;
    secondary_hex: string;
    accent_hex: string;
    font_family: string;
  };
  voice: {
    tone_adjectives: string[];
    audience_persona: string;
    writing_rules: string[];
    prohibited_words: string[];
    escalation_triggers: string[];
  };
  linked_account_ids: string[];
}

export interface CampaignBrief {
  campaign_id: string;
  brand_id: string;
  title: string;
  objective: string;
  target_platforms: string[];
  target_audience: string;
  content_formats: string[];
  hashtags: string[];
  start_date: string;
  end_date: string;
  status: string;
}

export interface CalendarPostSlot {
  slot_id: string;
  campaign_id?: string;
  brand_id: string;
  target_platform: string;
  scheduled_time: string;
  content_type: string;
  caption: string;
  hashtags: string[];
  is_approved: boolean;
  status: string;
}

export interface ToolCapability {
  name: string;
  command: string;
  is_available: boolean;
  version?: string;
  supported_features: string[];
}

export interface UnifiedInboxMessage {
  message_id: string;
  account_id: { 0: string };
  platform: string;
  author_handle: string;
  interaction_type: string;
  content_text: string;
  category: string;
  sentiment_score: number;
  drafted_reply?: string;
  is_auto_reply_eligible: boolean;
  requires_human_approval: boolean;
  status: string;
  received_at: string;
}

export interface VerifiedPlatformMetric {
  metric_id: string;
  platform: string;
  account_id: string;
  impressions: number;
  reach: number;
  views: number;
  engagement_rate: number;
  comments: number;
  shares: number;
  saves: number;
  conversions: number;
  is_verified_by_api: boolean;
  captured_at: string;
}

export interface SubAgentAssignment {
  role: string;
  title: string;
  current_task: string;
  assigned_model_profile: string;
  is_active: boolean;
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  private isTauriAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  stats = signal<SystemStats>({
    total_accounts: 24,
    active_automations: 8,
    pending_human_tasks: 2,
    system_status: 'ONLINE',
    updated_at: new Date().toISOString(),
  });

  hardware = signal<SystemHardwareMetrics>({
    cpu_percent: 14,
    gpu_percent: 22,
    memory_used_gb: 3.8,
    memory_total_gb: 16.0,
    disk_percent: 42,
  });

  activeOperations = signal<ActiveOperation[]>([
    {
      id: 'op-01',
      name: 'Instagram Trend Digest & Reel Generation',
      description: 'Synthesizing verified audio and compiling 9:16 vertical video reel with FFmpeg and Vector Craft',
      progress: 74,
      status: 'running',
      platform: 'instagram',
      started_at: '12 mins ago',
      current_step_index: 3,
      steps: [
        'Scan verified tech hashtags on Instagram Graph API',
        'Synthesize natural voiceover script via LM gateway',
        'Normalize audio levels to -14 LUFS',
        'Composite 1080x1920 graphic overlays with Vector Craft',
        'Stage for scheduled automated publication',
      ],
      logs: [
        '[12:00:14] Initialized task context with Argon2id session credentials',
        '[12:02:40] Voiceover track synthesized (28.4s, 48kHz, stereo)',
        '[12:05:12] EBU R128 audio loudness normalized to -14.1 LUFS',
        '[12:07:30] Compiling frame batch #1240-2400 via FFmpeg h264_nvenc...',
      ],
    },
    {
      id: 'op-02',
      name: 'Reddit Developer Discussion Engagement',
      description: 'Synthesizing technical response for r/devops offline cryptography thread within rate limits',
      progress: 45,
      status: 'running',
      platform: 'reddit',
      started_at: '18 mins ago',
      current_step_index: 2,
      steps: [
        'Scan subscribed technical subreddits for relevant inquiries',
        'Verify target thread complies with community guidelines',
        'Generate cryptographic technical explanation',
        'Await human operator gate approval before submission',
      ],
      logs: [
        '[11:58:02] Matched post query: "Best practices for local credential isolation"',
        '[12:01:10] Verified sub rules: link posts disallowed, technical code examples permitted',
        '[12:04:19] Formulated 320-word technical analysis with Argon2id citations',
      ],
    },
    {
      id: 'op-03',
      name: 'Nightly Argon2id Key Derivation & Vault Scrub',
      description: 'Zeroizing volatile memory buffers, rotating ephemeral slice salts, and flushing SQLite journals',
      progress: 100,
      status: 'completed',
      platform: 'vault',
      started_at: '1 hour ago',
      current_step_index: 4,
      steps: [
        'Enumerate locked credential records in local SQLite pool',
        'Re-derive AES-256-GCM master key with Argon2id parameters',
        'Perform zeroize_memory() on ephemeral plaintext buffers',
        'Write tamper-evident integrity hash to audit journal',
      ],
      logs: [
        '[11:15:00] Master vault integrity check started',
        '[11:15:04] Argon2id parameters verified: m=65536, t=3, p=4',
        '[11:15:08] All plaintext memory buffers successfully overwritten with zeros',
        '[11:15:09] Checkpoint journal signed: 0x9f4a...88c2',
      ],
    },
    {
      id: 'op-04',
      name: 'TikTok Short-Form Video Color Grading',
      description: 'Batch applying brand color LUTs and dynamic subtitle alignment via Photo Craft CLI',
      progress: 32,
      status: 'paused',
      platform: 'tiktok',
      started_at: '35 mins ago',
      current_step_index: 1,
      steps: [
        'Extract audio timestamps for kinetic subtitle tracking',
        'Apply high-contrast dark color palette in Photo Craft',
        'Encode H.265 vertical 60fps video stream',
      ],
      logs: [
        '[11:42:11] Subtitles parsed: 18 keyframes extracted',
        '[11:45:00] Operator triggered manual pause for asset inspection',
      ],
    },
  ]);

  notifications = signal<NotificationItem[]>([
    {
      id: 'notif-1',
      title: 'Human Gate Intervention Required',
      message: 'Cloudflare Turnstile challenge encountered during Instagram account registration preflight.',
      timestamp: '5 mins ago',
      type: 'security',
      read: false,
    },
    {
      id: 'notif-2',
      title: 'Video Asset Compilation Finished',
      message: 'FFmpeg rendered 1080x1920 (9:16) reel for Q4 Product Launch campaign.',
      timestamp: '22 mins ago',
      type: 'workflow',
      read: false,
    },
    {
      id: 'notif-3',
      title: 'Platform Rate Limit Sync',
      message: 'X developer tier limits verified. Backoff delay set to 60s per automated action.',
      timestamp: '45 mins ago',
      type: 'policy',
      read: true,
    },
    {
      id: 'notif-4',
      title: 'Vault Cryptographic Scrub Complete',
      message: 'Offline memory buffers zeroized. Argon2id salt refreshed without errors.',
      timestamp: '1 hour ago',
      type: 'system',
      read: true,
    },
  ]);

  accounts = signal<AccountRecord[]>([
    {
      id: 'acc-1',
      platform: 'instagram',
      username: 'growth_brand_ai',
      email: 'instagram_01@openaccounts.local',
      status: 'active',
      health_score: 98,
      created_at: '2026-10-15T10:00:00Z',
      last_active: '2 mins ago',
    },
    {
      id: 'acc-2',
      platform: 'reddit',
      username: 'u/AutoDevDesk',
      email: 'reddit_dev@openaccounts.local',
      status: 'active',
      health_score: 95,
      created_at: '2026-10-18T14:30:00Z',
      last_active: '5 mins ago',
    },
    {
      id: 'acc-3',
      platform: 'youtube',
      username: 'StudioContentSync',
      email: 'yt_sync@openaccounts.local',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-20T08:00:00Z',
      last_active: '1 hour ago',
    },
    {
      id: 'acc-4',
      platform: 'tiktok',
      username: '@DevOpsShorts',
      email: 'tiktok_ops@openaccounts.local',
      status: 'active',
      health_score: 92,
      created_at: '2026-10-22T19:00:00Z',
      last_active: '15 mins ago',
    },
    {
      id: 'acc-5',
      platform: 'x',
      username: '@OpenAccountsCore',
      email: 'x_brand@openaccounts.local',
      status: 'active',
      health_score: 94,
      created_at: '2026-10-25T11:20:00Z',
      last_active: 'Just now',
    },
    {
      id: 'acc-6',
      platform: 'linkedin',
      username: 'openaccounts-platform',
      email: 'li_corp@openaccounts.local',
      status: 'active',
      health_score: 97,
      created_at: '2026-10-26T09:40:00Z',
      last_active: '3 hours ago',
    },
    {
      id: 'acc-7',
      platform: 'github',
      username: 'openaccounts-robot',
      email: 'gh_bot@openaccounts.local',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-12T16:10:00Z',
      last_active: '20 mins ago',
    },
    {
      id: 'acc-8',
      platform: 'discord',
      username: 'OpenAccountsNotify#4412',
      email: 'dc_notify@openaccounts.local',
      status: 'active',
      health_score: 96,
      created_at: '2026-10-28T13:00:00Z',
      last_active: '10 mins ago',
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

  brands = signal<BrandProfile[]>([
    {
      brand_id: 'brand-01',
      name: 'OpenAccounts AI',
      tagline: 'Autonomous Account Infrastructure',
      visual_identity: {
        primary_hex: '#141722',
        secondary_hex: '#0b0d12',
        accent_hex: '#d946ef',
        font_family: 'Inter, sans-serif',
      },
      voice: {
        tone_adjectives: ['Authoritative', 'Innovative', 'Pragmatic'],
        audience_persona: 'Engineers, marketers, and technical founders requiring automated scale.',
        writing_rules: ['Direct statements', 'Highlight offline cryptography', 'Never over-promise'],
        prohibited_words: ['quick cash', 'spam bot', 'guaranteed clicks'],
        escalation_triggers: ['legal', 'vulnerability', 'chargeback', 'abuse'],
      },
      linked_account_ids: ['acc-1', 'acc-3', 'acc-5'],
    },
  ]);

  campaigns = signal<CampaignBrief[]>([
    {
      campaign_id: 'camp-01',
      brand_id: 'brand-01',
      title: 'Q4 Product Launch — Autonomous Account Workforce',
      objective: 'Brand Awareness & Technical Adoption',
      target_platforms: ['Instagram', 'YouTube', 'TikTok', 'X'],
      target_audience: 'Technical creators and operations teams',
      content_formats: ['Reels (9:16)', 'Shorts (9:16)', 'X Threads'],
      hashtags: ['#OpenAccounts', '#DevTools', '#AIAutomation', '#TechReels'],
      start_date: '2026-11-01',
      end_date: '2026-11-30',
      status: 'Active',
    },
    {
      campaign_id: 'camp-02',
      brand_id: 'brand-01',
      title: 'Developer Security & Offline Vault Showcase',
      objective: 'Educate security community on Argon2id buffer isolation',
      target_platforms: ['Reddit', 'YouTube', 'LinkedIn', 'GitHub'],
      target_audience: 'DevSecOps engineers and cybersecurity architects',
      content_formats: ['Deep-dive Video', 'Reddit Case Study', 'GitHub Guide'],
      hashtags: ['#Argon2id', '#CyberSecurity', '#RustLang', '#PrivacyFirst'],
      start_date: '2026-11-05',
      end_date: '2026-12-15',
      status: 'Planned',
    },
  ]);

  calendarPosts = signal<CalendarPostSlot[]>([
    {
      slot_id: 'slot-101',
      campaign_id: 'camp-01',
      brand_id: 'brand-01',
      target_platform: 'Instagram',
      scheduled_time: 'Tomorrow, 10:00 AM',
      content_type: 'Reel (9:16 Vertical)',
      caption: 'Meet your new autonomous social media department. Built-in Vector Craft, Photo Craft, and FFmpeg integration. 🚀',
      hashtags: ['#OpenAccounts', '#AIAutomation', '#Reels'],
      is_approved: true,
      status: 'Scheduled',
    },
    {
      slot_id: 'slot-102',
      campaign_id: 'camp-01',
      brand_id: 'brand-01',
      target_platform: 'TikTok',
      scheduled_time: 'Tomorrow, 2:30 PM',
      content_type: 'Short Video (9:16)',
      caption: 'Why manual account operations are dead in 2026. Watch full explainer.',
      hashtags: ['#TechTok', '#CodingLife', '#Automation'],
      is_approved: false,
      status: 'AwaitingApproval',
    },
    {
      slot_id: 'slot-103',
      campaign_id: 'camp-01',
      brand_id: 'brand-01',
      target_platform: 'YouTube',
      scheduled_time: 'Nov 3, 9:00 AM',
      content_type: 'Shorts (9:16)',
      caption: 'Offline encrypted vault + multi-account isolation in action.',
      hashtags: ['#YouTubeShorts', '#CyberSecurity'],
      is_approved: true,
      status: 'Scheduled',
    },
  ]);

  creativeTools = signal<ToolCapability[]>([
    {
      name: 'Vector Craft',
      command: 'vectorcraft-cli',
      is_available: true,
      version: 'v0.11.0',
      supported_features: ['Vector Graphics', 'SVG/PDF Rendering', 'Logo & Badge Generation'],
    },
    {
      name: 'Photo Craft',
      command: 'photocraft-cli',
      is_available: true,
      version: 'v0.9.0',
      supported_features: ['Image Editing', 'Layer Compositing', 'Smart Sharpen', 'Batch Color Grade'],
    },
    {
      name: 'OpenToonz',
      command: 'tcomposer.exe',
      is_available: true,
      version: 'v1.5.0',
      supported_features: ['2D Character Animation', 'Scene Rendering', 'Frame Cleanup'],
    },
    {
      name: 'Effect Craft',
      command: 'effectcraft-cli',
      is_available: true,
      version: 'v0.10.0',
      supported_features: ['Motion Graphics', 'Lottie Export', 'Video Compositing', 'Transitions'],
    },
    {
      name: 'Blender',
      command: 'blender.exe',
      is_available: true,
      version: 'v5.1.2',
      supported_features: ['3D Modeling', 'Cinematic Camera', 'Eevee & Cycles Render'],
    },
    {
      name: 'FFmpeg CLI',
      command: 'ffmpeg',
      is_available: true,
      version: 'v9.0.2',
      supported_features: ['Transcoding', '9:16 Reel Resizing', 'Audio Normalization (-14 LUFS)', 'Subtitles'],
    },
  ]);

  inboxMessages = signal<UnifiedInboxMessage[]>([
    {
      message_id: 'inb-101',
      account_id: { 0: 'acc-1' },
      platform: 'instagram',
      author_handle: '@creator_studio',
      interaction_type: 'comment',
      content_text: 'Does OpenAccounts support scheduling 9:16 Instagram Reels automatically?',
      category: 'sales_lead',
      sentiment_score: 0.85,
      drafted_reply: 'Yes! OpenAccounts includes full 9:16 Reel rendering, audio normalization, and automated publishing via official Graph API.',
      is_auto_reply_eligible: true,
      requires_human_approval: false,
      status: 'New',
      received_at: '2026-10-31 20:15:00',
    },
    {
      message_id: 'inb-102',
      account_id: { 0: 'acc-5' },
      platform: 'x',
      author_handle: '@enterprise_buyer',
      interaction_type: 'direct_message',
      content_text: 'We want to purchase an enterprise license and require custom legal terms for our multi-account setup.',
      category: 'security_or_legal_escalation',
      sentiment_score: 0.5,
      drafted_reply: 'Thank you for reaching out. A human account executive has been notified to provide custom licensing terms.',
      is_auto_reply_eligible: false,
      requires_human_approval: true,
      status: 'PendingApproval',
      received_at: '2026-10-31 20:20:00',
    },
  ]);

  verifiedMetrics = signal<VerifiedPlatformMetric[]>([
    {
      metric_id: 'm-01',
      platform: 'instagram',
      account_id: 'growth_brand_ai',
      impressions: 48200,
      reach: 39500,
      views: 28400,
      engagement_rate: 6.8,
      comments: 312,
      shares: 124,
      saves: 840,
      conversions: 45,
      is_verified_by_api: true,
      captured_at: '2026-10-31 20:00:00',
    },
    {
      metric_id: 'm-02',
      platform: 'youtube',
      account_id: 'StudioContentSync',
      impressions: 112000,
      reach: 89000,
      views: 74200,
      engagement_rate: 8.2,
      comments: 540,
      shares: 410,
      saves: 1200,
      conversions: 89,
      is_verified_by_api: true,
      captured_at: '2026-10-31 20:00:00',
    },
    {
      metric_id: 'm-03',
      platform: 'tiktok',
      account_id: '@DevOpsShorts',
      impressions: 86500,
      reach: 71000,
      views: 59300,
      engagement_rate: 9.4,
      comments: 420,
      shares: 615,
      saves: 1890,
      conversions: 62,
      is_verified_by_api: true,
      captured_at: '2026-10-31 20:10:00',
    },
  ]);

  nimCascadeModels = signal<string[]>([
    'nvidia/nemotron-3.5-super-vl-120b-a12b',
    'meta/llama-3.2-90b-vision-instruct',
    'meta/muse-glimmer-30b',
    'nvidia/nemotron-3.5-lightning-30b-a3b',
    'nvidia/nemotron-3-nano-omni-30b-a3b-reasoning',
    'openai/gpt-oss-20b',
    'minimaxai/minimax-m3.1-flash',
    'poolside/laguna-xs-2.1',
    'stepfun-ai/step-5-preview',
    'nvidia/nemotron-3-ultra-550b-a55b',
    'mistralai/mistral-large-4-1t-a52b',
    'nvidia/nemotron-3.5-bolt-e4b',
  ]);

  collaboratingAgents = signal<SubAgentAssignment[]>([
    { role: 'ContentStrategist', title: 'Head of Marketing Strategy', current_task: 'Synthesizing cross-platform audience briefs', assigned_model_profile: 'NIM: nvidia/nemotron-3.5-super-vl-120b-a12b', is_active: true },
    { role: 'TrendResearcher', title: 'Live Trend Intelligence Agent', current_task: 'Scanning viral hashtag velocity on TikTok and X', assigned_model_profile: 'NIM: nvidia/nemotron-3-nano-omni-30b-a3b-reasoning', is_active: true },
    { role: 'GraphicDesigner', title: 'Vector & Photo Production Specialist', current_task: 'Driving Vector Craft CLI & Photo Craft CLI', assigned_model_profile: 'NIM Vision: meta/llama-3.2-90b-vision-instruct', is_active: true },
    { role: 'VideoDirector', title: 'Short-Form Video & Reel Director', current_task: 'Assembling 9:16 vertical reels with FFmpeg & Effect Craft', assigned_model_profile: 'NIM: meta/muse-glimmer-30b', is_active: true },
    { role: 'CommunityManager', title: 'Community Moderation & Engagement', current_task: 'Classifying incoming audience comments and inquiries', assigned_model_profile: 'NIM: nvidia/nemotron-3.5-lightning-30b-a3b', is_active: true },
    { role: 'ComplianceAgent', title: 'Policy & Rate Limit Gatekeeper', current_task: 'Enforcing official Graph and X API compliance limits', assigned_model_profile: 'Deterministic Policy Engine', is_active: true },
  ]);

  isAutonomousLoopRunning = signal<boolean>(true);
  vaultUnlocked = signal<boolean>(false);
  killSwitchActive = signal<boolean>(false);

  toggleOperationPause(id: string) {
    this.activeOperations.update((ops) =>
      ops.map((op) => {
        if (op.id === id) {
          const nextStatus = op.status === 'running' ? 'paused' : 'running';
          return { ...op, status: nextStatus };
        }
        return op;
      })
    );
  }

  retryOperation(id: string) {
    this.activeOperations.update((ops) =>
      ops.map((op) => {
        if (op.id === id) {
          return {
            ...op,
            status: 'running',
            progress: Math.min(100, op.progress + 15),
            logs: [...op.logs, `[${new Date().toLocaleTimeString()}] Operator triggered retry sequence`],
          };
        }
        return op;
      })
    );
  }

  markAllNotificationsRead() {
    this.notifications.update((items) => items.map((i) => ({ ...i, read: true })));
  }

  clearNotifications() {
    this.notifications.set([]);
  }

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
      case 'list_brands':
        return this.brands() as unknown as T;
      case 'list_campaigns':
        return this.campaigns() as unknown as T;
      case 'list_calendar_posts':
        return this.calendarPosts() as unknown as T;
      case 'list_creative_tools':
        return this.creativeTools() as unknown as T;
      case 'list_unified_inbox':
        return this.inboxMessages() as unknown as T;
      case 'approve_and_send_inbox_reply': {
        const messageId = args?.['message_id'] as string;
        this.inboxMessages.update((msgs) =>
          msgs.map((m) =>
            m.message_id === messageId
              ? { ...m, status: 'Replied', drafted_reply: args?.['final_reply'] as string }
              : m
          )
        );
        return undefined as unknown as T;
      }
      case 'list_social_analytics':
        return this.verifiedMetrics() as unknown as T;
      case 'list_collaborating_agents':
        return this.collaboratingAgents() as unknown as T;
      case 'start_autonomous_marketing_loop':
        this.isAutonomousLoopRunning.set(true);
        return undefined as unknown as T;
      case 'pause_autonomous_marketing_loop':
        this.isAutonomousLoopRunning.set(false);
        return undefined as unknown as T;
      case 'is_autonomous_marketing_loop_running':
        return this.isAutonomousLoopRunning() as unknown as T;
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

  async toggleMarketingLoop() {
    if (this.isAutonomousLoopRunning()) {
      await this.invoke('pause_autonomous_marketing_loop');
    } else {
      await this.invoke('start_autonomous_marketing_loop');
    }
  }
}
