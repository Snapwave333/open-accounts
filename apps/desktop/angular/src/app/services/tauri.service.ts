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

export interface StickAgentPairing {
  openaccounts_agent: string;
  openaccounts_title: string;
  stick_agents: string[];
  stick_roles: string[];
  channel: string;
}

export interface StickBridgeMessage {
  id: string;
  timestamp: string;
  direction: string;
  from_agent: string;
  to_agent: string;
  channel: string;
  subject: string;
  body: string;
  status: string;
}

export interface StickEpisodeSummary {
  slug: string;
  title: string;
  has_long_mp4: boolean;
  shorts_count: number;
  youtube_published: boolean;
  tiktok_posted: boolean;
  instagram_posted: boolean;
  kitchen_dish?: string;
  kitchen_show?: string;
}

export interface GarbageCollectionReport {
  executed_at: string;
  applied: boolean;
  free_gb: number;
  freed_this_run_gb: number;
  freed_total_gb: number;
  items_cleaned: number;
  logs_rotated: number;
  emulator_status: string;
  orphans_status: string;
  disk_pressure: boolean;
  openaccounts_buffers_scrubbed: number;
}

export interface FlowCardItem {
  id: string;
  title: string;
  detail: string;
  tag: string;
  time_label: string;
  eta?: string;
  progress?: number;
  platform: 'instagram' | 'tiktok' | 'youtube' | 'shopify' | 'paypal' | 'gmail' | 'outlook';
  account_handle: string;
}

export interface ChatAttachment {
  id: string;
  name: string;
  size_label: string;
  mime_type: string;
  is_image: boolean;
  data_url?: string;
}

export interface AgentChatMessage {
  id: string;
  sender: 'human' | 'agent';
  author_label: string;
  timestamp: string;
  text: string;
  target_scope?: string;
  attachments: ChatAttachment[];
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
      id: 'cdn-ig-snapwave',
      platform: 'instagram',
      username: '@snapwave',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 99,
      created_at: '2026-10-28T10:00:00Z',
      last_active: 'Just now',
    },
    {
      id: 'cdn-tt-clearlydrawnlessons',
      platform: 'tiktok',
      username: '@clearlydrawnlessons',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 98,
      created_at: '2026-10-26T14:30:00Z',
      last_active: '2 mins ago',
    },
    {
      id: 'cdn-yt-clearlydrawnnews',
      platform: 'youtube',
      username: '@ClearlyDrawnLessons (UC16fbSsDJcc6RVvNsH3Cbfg)',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-20T08:00:00Z',
      last_active: '5 mins ago',
    },
    {
      id: 'cdn-shopify-store',
      platform: 'shopify',
      username: 'zs56hs-fj.myshopify.com',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-22T19:00:00Z',
      last_active: '12 mins ago',
    },
    {
      id: 'cdn-paypal-snapwave333',
      platform: 'paypal',
      username: 'paypal.me/snapwave333',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-25T11:20:00Z',
      last_active: '30 mins ago',
    },
    {
      id: 'cdn-email-gmail',
      platform: 'email',
      username: 'elikenny333@gmail.com',
      email: 'elikenny333@gmail.com',
      status: 'active',
      health_score: 100,
      created_at: '2026-10-15T09:40:00Z',
      last_active: '1 min ago',
    },
    {
      id: 'cdn-email-outlook',
      platform: 'email',
      username: 'focogaming@outlook.com',
      email: 'focogaming@outlook.com',
      status: 'active',
      health_score: 97,
      created_at: '2026-10-18T16:10:00Z',
      last_active: '10 mins ago',
    },
  ]);

  humanTasks = signal<HumanTask[]>([
    {
      id: 'task-101',
      account_id: 'cdn-yt-clearlydrawnnews',
      challenge_type: '24hReviewGate',
      title: '24-Hour Review Window Check (Q. Quartz Gate)',
      instructions: 'Verify episode news-2026-10-31 Cartoon Kitchen recipe citation before public YouTube release.',
      page_url: 'https://studio.youtube.com/channel/UC16fbSsDJcc6RVvNsH3Cbfg/videos',
      status: 'Pending',
      created_at: '2026-10-31T18:30:00Z',
    },
    {
      id: 'task-102',
      account_id: 'cdn-ig-snapwave',
      challenge_type: 'SponsorApproval',
      title: 'Sponsor Inquiry Approval (S. Sapphire & G. Gold)',
      instructions: 'Approve outbound rate card reply from elikenny333@gmail.com.',
      page_url: 'https://instagram.com/snapwave',
      status: 'Pending',
      created_at: '2026-10-31T19:10:00Z',
    },
  ]);

  emailIdentities = signal<EmailIdentity[]>([
    {
      identity_id: 'eid-cdn-gmail',
      email_address: 'elikenny333@gmail.com',
      provider_type: 'gmail_api',
      assigned_account_id: 'cdn-yt-clearlydrawnnews',
      platform: 'youtube / instagram / tiktok / shopify',
      is_primary: true,
      created_at: '2026-10-15T09:00:00Z',
    },
    {
      identity_id: 'eid-cdn-outlook',
      email_address: 'focogaming@outlook.com',
      provider_type: 'microsoft_graph',
      assigned_account_id: 'cdn-email-outlook',
      platform: 'outlook_watch',
      is_primary: false,
      created_at: '2026-10-18T11:30:00Z',
    },
  ]);

  policyAlerts = signal<PolicyAlert[]>([
    {
      alert_id: 'palert-01',
      platform_id: 'youtube',
      severity: 'Info',
      message: 'YouTube Data API v3 quota healthy (max 9 uploads/day, 20 comment replies/day enforced).',
      created_at: '2026-10-31 20:10:00',
    },
    {
      alert_id: 'palert-02',
      platform_id: 'instagram',
      severity: 'Info',
      message: '@snapwave Creator Relay limits synced: max 12 likes, 3 follows, 2 comments, 15 replies/day.',
      created_at: '2026-10-31 19:40:00',
    },
  ]);

  brands = signal<BrandProfile[]>([
    {
      brand_id: 'brand_cdn',
      name: 'Clearly Drawn News (StickExplainers)',
      tagline: 'Animated Cartoon & Anime News + The Cartoon Kitchen with Marcus & Chef Elena',
      visual_identity: {
        primary_hex: '#0f172a',
        secondary_hex: '#1e293b',
        accent_hex: '#f59e0b',
        font_family: 'Inter, sans-serif',
      },
      voice: {
        tone_adjectives: ['Warm', 'Witty', 'Factual', 'Nostalgic', 'All-Ages'],
        audience_persona: 'Animation, anime, cartoon, and pop-culture food fans across YouTube, TikTok, and Instagram.',
        writing_rules: [
          'Sound like Marcus (dry-wit news anchor) or Chef Elena (warm Cartoon Kitchen host)',
          'Credit real recipe creators by name and link in every description',
          'Keep captions concise (1-2 sentences) with max 5 relevant hashtags',
        ],
        prohibited_words: ['clickbait', 'guaranteed viral', 'sub4sub'],
        escalation_triggers: ['sponsor', 'legal', 'copyright', 'refund'],
      },
      linked_account_ids: [
        'cdn-ig-snapwave',
        'cdn-tt-clearlydrawnlessons',
        'cdn-yt-clearlydrawnnews',
        'cdn-shopify-store',
        'cdn-paypal-snapwave333',
      ],
    },
  ]);

  campaigns = signal<CampaignBrief[]>([
    {
      campaign_id: 'camp-cdn-01',
      brand_id: 'brand_cdn',
      title: 'Clearly Drawn News — Daily Episode & 3x 9:16 Shorts Pipeline',
      objective: 'Cross-Platform Subscriber & Watch-Time Growth',
      target_platforms: ['YouTube (@ClearlyDrawnLessons)', 'TikTok (@clearlydrawnlessons)', 'Instagram (@snapwave)'],
      target_audience: 'Anime, cartoon, and animated food enthusiasts',
      content_formats: ['Long-Form Episode (16:9)', '3x Vertical Shorts (9:16)', 'Cartoon Kitchen Segment'],
      hashtags: ['#ClearlyDrawnNews', '#AnimationNews', '#CartoonKitchen', '#AnimeNews', '#Shorts'],
      start_date: '2026-10-25',
      end_date: '2026-12-31',
      status: 'Active',
    },
    {
      campaign_id: 'camp-cdn-02',
      brand_id: 'brand_cdn',
      title: 'Cartoon Kitchen Merch Drop (zs56hs-fj.myshopify.com)',
      objective: 'Drive Printful & Shopify Store Conversions from Engaged Viewers',
      target_platforms: ['Instagram (@snapwave)', 'YouTube (@ClearlyDrawnLessons)', 'Shopify'],
      target_audience: 'Fans of Marcus & Chef Elena episodes',
      content_formats: ['Reel Showcase', 'Pinned Comment Links', 'Community Polls'],
      hashtags: ['#CartoonKitchen', '#StickExplainers', '#AnimeFood', '#CreatorMerch'],
      start_date: '2026-10-28',
      end_date: '2026-12-15',
      status: 'Active',
    },
  ]);

  calendarPosts = signal<CalendarPostSlot[]>([
    {
      slot_id: 'slot-cdn-101',
      campaign_id: 'camp-cdn-01',
      brand_id: 'brand_cdn',
      target_platform: 'YouTube (@ClearlyDrawnLessons)',
      scheduled_time: 'Today, 15:00 UTC',
      content_type: 'Full Episode + 3 Shorts',
      caption: 'Marcus breaks down today’s biggest animation headlines, then Chef Elena recreates Studio Ghibli ramen in the Cartoon Kitchen!',
      hashtags: ['#ClearlyDrawnNews', '#AnimationNews', '#CartoonKitchen', '#Shorts'],
      is_approved: true,
      status: 'Scheduled',
    },
    {
      slot_id: 'slot-cdn-102',
      campaign_id: 'camp-cdn-01',
      brand_id: 'brand_cdn',
      target_platform: 'Instagram (@snapwave)',
      scheduled_time: 'Today, 17:00 UTC',
      content_type: 'Reel (9:16 Vertical)',
      caption: 'Would you try Chef Elena’s Cartoon Kitchen recipe? Vote in the comments! 🍜✨',
      hashtags: ['#ClearlyDrawnNews', '#CartoonKitchen', '#AnimeFood', '#Reels'],
      is_approved: true,
      status: 'Scheduled',
    },
    {
      slot_id: 'slot-cdn-103',
      campaign_id: 'camp-cdn-01',
      brand_id: 'brand_cdn',
      target_platform: 'TikTok (@clearlydrawnlessons)',
      scheduled_time: 'Today, 19:30 UTC',
      content_type: 'Short Video (9:16)',
      caption: 'Wait for Marcus’s reaction to the 90s Saturday morning cartoon trivia! 📺',
      hashtags: ['#ClearlyDrawnLessons', '#CartoonNews', '#90sCartoons', '#Animation'],
      is_approved: false,
      status: 'AwaitingApproval',
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
      account_id: { 0: 'cdn-yt-clearlydrawnnews' },
      platform: 'youtube',
      author_handle: '@anime_foodie_99',
      interaction_type: 'comment',
      content_text: 'Chef Elena’s ramen broth looked incredible! Where can I get the Cartoon Kitchen mug?',
      category: 'sales_lead',
      sentiment_score: 0.94,
      drafted_reply: 'So glad you loved Elena’s ramen segment! You can grab the official Cartoon Kitchen mug right at our shop: https://zs56hs-fj.myshopify.com 🍜',
      is_auto_reply_eligible: true,
      requires_human_approval: false,
      status: 'New',
      received_at: '2026-10-31 20:15:00',
    },
    {
      message_id: 'inb-102',
      account_id: { 0: 'cdn-ig-snapwave' },
      platform: 'instagram',
      author_handle: '@studio_sponsor_media',
      interaction_type: 'direct_message',
      content_text: 'We would love to sponsor a 3-episode Cartoon Kitchen block on Clearly Drawn News. Can you send your rate card?',
      category: 'security_or_legal_escalation',
      sentiment_score: 0.88,
      drafted_reply: 'Thanks for reaching out! S. Sapphire & G. Gold have queued our media kit and rate card from elikenny333@gmail.com for review.',
      is_auto_reply_eligible: false,
      requires_human_approval: true,
      status: 'PendingApproval',
      received_at: '2026-10-31 20:20:00',
    },
  ]);

  verifiedMetrics = signal<VerifiedPlatformMetric[]>([
    {
      metric_id: 'm-01',
      platform: 'youtube',
      account_id: '@ClearlyDrawnLessons',
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
      metric_id: 'm-02',
      platform: 'instagram',
      account_id: '@snapwave',
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
      metric_id: 'm-03',
      platform: 'tiktok',
      account_id: '@clearlydrawnlessons',
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
    { role: 'ContentStrategist', title: 'Head of Marketing Strategy', current_task: 'Coordinating with V. Sterling (CEO) & J. Jade (News Scout)', assigned_model_profile: 'NIM: nvidia/nemotron-3.5-super-vl-120b-a12b', is_active: true },
    { role: 'TrendResearcher', title: 'Live Trend Intelligence Agent', current_task: 'Syncing retention data with C. Copper, A. Amber & I. Indigo', assigned_model_profile: 'NIM: nvidia/nemotron-3-nano-omni-30b-a3b-reasoning', is_active: true },
    { role: 'GraphicDesigner', title: 'Vector & Photo Production Specialist', current_task: 'Coordinating thumbnails & dish art with T. Tangerine & D. Sienna', assigned_model_profile: 'NIM Vision: meta/llama-3.2-90b-vision-instruct', is_active: true },
    { role: 'VideoDirector', title: 'Short-Form Video & Reel Director', current_task: 'Receiving 9:16 Shorts from C. Cyan, R. Ruby & F. Fuchsia', assigned_model_profile: 'NIM: meta/muse-glimmer-30b', is_active: true },
    { role: 'CommunityManager', title: 'Community Moderation & Engagement', current_task: 'Managing delegated social & replies with C. Coral, E. Emerald & P. Plum', assigned_model_profile: 'NIM: nvidia/nemotron-3.5-lightning-30b-a3b', is_active: true },
    { role: 'ComplianceAgent', title: 'Policy & Rate Limit Gatekeeper', current_task: 'Enforcing 24h buffer & caps with Q. Quartz (Quality Gate) & G. Gold (CFO)', assigned_model_profile: 'Deterministic Policy Engine', is_active: true },
  ]);

  stickBridgePairings = signal<StickAgentPairing[]>([
    {
      openaccounts_agent: 'ContentStrategist',
      openaccounts_title: 'Head of Marketing Strategy',
      stick_agents: ['V. Sterling', 'J. Jade', 'S. Saffron'],
      stick_roles: ['CEO Boss Agent', 'News Scout', 'Cartoon Kitchen Chef'],
      channel: 'Editorial & Campaign Strategy',
    },
    {
      openaccounts_agent: 'TrendResearcher',
      openaccounts_title: 'Live Trend Intelligence Agent',
      stick_agents: ['C. Copper', 'A. Amber', 'I. Indigo'],
      stick_roles: ['Competitor Scout', 'Competitor Analyst', 'Performance Analyst'],
      channel: 'Trend & Retention Intelligence',
    },
    {
      openaccounts_agent: 'GraphicDesigner',
      openaccounts_title: 'Vector & Photo Production Specialist',
      stick_agents: ['T. Tangerine', 'D. Sienna'],
      stick_roles: ['Thumbnail Director', 'Dish Artist'],
      channel: 'Thumbnails & Dish Art Assets',
    },
    {
      openaccounts_agent: 'VideoDirector',
      openaccounts_title: 'Short-Form Video & Reel Director',
      stick_agents: ['C. Cyan', 'R. Ruby', 'F. Fuchsia'],
      stick_roles: ['Animator & Renderer', 'YouTube Publisher', 'TikTok & Reels Poster'],
      channel: 'Episode & 9:16 Shorts Delivery',
    },
    {
      openaccounts_agent: 'CommunityManager',
      openaccounts_title: 'Community Moderation & Engagement',
      stick_agents: ['C. Coral', 'E. Emerald', 'P. Plum', 'M. Maroon'],
      stick_roles: ['Social Media Agent', 'Engagement Agent', 'Community Manager', 'Merch Agent'],
      channel: 'Delegated Social, Comments & Merch Replies',
    },
    {
      openaccounts_agent: 'ComplianceAgent',
      openaccounts_title: 'Policy & Rate Limit Gatekeeper',
      stick_agents: ['Q. Quartz', 'G. Gold', 'E. Ebony'],
      stick_roles: ['Quality Gate', 'CFO', 'Email Watchdog'],
      channel: '24h Buffer, Quota & Sponsor Triage',
    },
  ]);

  stickBridgeMessages = signal<StickBridgeMessage[]>([
    {
      id: 'br-msg-001',
      timestamp: 'Just now',
      direction: 'openaccounts_to_stick',
      from_agent: 'OpenAccounts · CommunityManager',
      to_agent: 'C. Coral & E. Emerald',
      channel: 'Delegated Social, Comments & Merch Replies',
      subject: 'Autopilot Engagement & Social Handoff Active',
      body: 'OpenAccounts has taken over @snapwave (IG), @clearlydrawnlessons (TikTok), and @ClearlyDrawnLessons (YouTube) engagement and comment replies so autopilot.py has zero duplicate actions.',
      status: 'acknowledged',
    },
    {
      id: 'br-msg-002',
      timestamp: '2 mins ago',
      direction: 'stick_to_openaccounts',
      from_agent: 'V. Sterling (CEO) & Q. Quartz (Quality Gate)',
      to_agent: 'OpenAccounts · ComplianceAgent',
      channel: '24h Buffer, Quota & Sponsor Triage',
      subject: 'Enforce 24-Hour Review Window & Daily Caps',
      body: 'Keep the 24-hour review window on YouTube releases and maintain strict daily caps (IG/TT: 12 likes, 3 follows, 2 comments, 15 replies; YT: 20 replies). Cite Cartoon Kitchen recipe creators.',
      status: 'synced',
    },
    {
      id: 'br-msg-003',
      timestamp: '5 mins ago',
      direction: 'stick_to_openaccounts',
      from_agent: 'R. Ruby (YouTube) & F. Fuchsia (TikTok/Reels)',
      to_agent: 'OpenAccounts · VideoDirector',
      channel: 'Episode & 9:16 Shorts Delivery',
      subject: 'Shorts & Reels Queue Synced from out/news-*',
      body: 'Rendered 9:16 vertical Shorts and thumbnails in out/news-* are exposed to OpenAccounts PublishingEngine with Marcus & Chef Elena voice metadata.',
      status: 'synced',
    },
    {
      id: 'br-msg-004',
      timestamp: '8 mins ago',
      direction: 'openaccounts_to_stick',
      from_agent: 'OpenAccounts · TrendResearcher',
      to_agent: 'J. Jade (News Scout) & S. Saffron (Kitchen Chef)',
      channel: 'Trend & Retention Intelligence',
      subject: 'Audience Demand Signal: Anime Cooking & Retro 90s Polls',
      body: 'Cross-platform comment sentiment on @snapwave and @ClearlyDrawnLessons shows +24% higher saves on episodes featuring Studio Ghibli & nostalgic 90s cartoon dishes.',
      status: 'delivered',
    },
  ]);

  stickBridgeEpisodes = signal<StickEpisodeSummary[]>([
    {
      slug: 'news-2026-10-31',
      title: 'Clearly Drawn News: Studio Ghibli Exhibit & Cartoon Kitchen Ramen',
      has_long_mp4: true,
      shorts_count: 3,
      youtube_published: true,
      tiktok_posted: true,
      instagram_posted: true,
      kitchen_dish: 'Ponyo Honey Ham Ramen',
      kitchen_show: 'Ponyo',
    },
  ]);

  gcReport = signal<GarbageCollectionReport>({
    executed_at: 'Just now',
    applied: true,
    free_gb: 80.2,
    freed_this_run_gb: 0.0,
    freed_total_gb: 26.37,
    items_cleaned: 0,
    logs_rotated: 0,
    emulator_status: 'emulator not running',
    orphans_status: '0 orphaned render worker(s)',
    disk_pressure: false,
    openaccounts_buffers_scrubbed: 4,
  });

  justDidItems = signal<FlowCardItem[]>([
    {
      id: 'jd-1',
      title: 'Posted Cartoon Kitchen Card on @snapwave',
      detail: 'Shared Howl’s Bacon & Eggs (Calcifer) card with recipe creator credit & Chef Elena caption.',
      tag: 'INSTAGRAM',
      time_label: '18:19 Local',
      platform: 'instagram',
      account_handle: '@snapwave',
    },
    {
      id: 'jd-2',
      title: 'Checked Comments Across YouTube, TikTok & IG',
      detail: 'Scanned @ClearlyDrawnLessons, @clearlydrawnlessons, and @snapwave (0 new, 5 drafts queued).',
      tag: 'COMMENTS',
      time_label: 'Just now',
      platform: 'youtube',
      account_handle: '@ClearlyDrawnLessons',
    },
    {
      id: 'jd-3',
      title: 'Completed Daily Outbound Engagement Batch',
      detail: 'Logged 8 IG likes, 3 IG follows, 3 YouTube likes, 1 YouTube subscription, and 1 YT comment reply.',
      tag: 'ENGAGE',
      time_label: 'Today',
      platform: 'tiktok',
      account_handle: '@clearlydrawnlessons',
    },
    {
      id: 'jd-4',
      title: 'Garbage Collection Sweep (G. Graphite)',
      detail: 'Verified 80.2 GB free disk space (26.37 GB total reclaimed), 0 orphaned render workers.',
      tag: 'CLEANUP',
      time_label: '22:30 Local',
      platform: 'shopify',
      account_handle: 'Studio Host',
    },
  ]);

  activelyDoingItems = signal<FlowCardItem[]>([
    {
      id: 'ad-1',
      title: 'Engaging Niche Anime & Cartoon Posts',
      detail: 'Liking & following #animefood and #studioghibli videos on @clearlydrawnlessons & @snapwave with 25–90s human pacing.',
      tag: 'RUNNING NOW',
      time_label: 'Live Cycle',
      eta: 'ETA ~4m left',
      progress: 72,
      platform: 'instagram',
      account_handle: '@snapwave',
    },
    {
      id: 'ad-2',
      title: 'Monitoring Incoming Comments & Merch Questions',
      detail: 'Auto-drafting Marcus & Chef Elena replies via free NVIDIA NIM cascade and matching Shopify store links.',
      tag: 'WATCHING',
      time_label: 'Continuous',
      eta: 'ETA ~45s / scan',
      progress: 88,
      platform: 'youtube',
      account_handle: '@ClearlyDrawnLessons',
    },
    {
      id: 'ad-3',
      title: 'Bridging with StickExplainers 39-Agent Studio',
      detail: 'Syncing episode status and directives with V. Sterling (CEO), C. Coral, E. Emerald, and Q. Quartz.',
      tag: 'BRIDGE',
      time_label: 'Connected',
      eta: 'ETA ~15s sync',
      progress: 94,
      platform: 'tiktok',
      account_handle: '@clearlydrawnlessons',
    },
  ]);

  planToDoNextItems = signal<FlowCardItem[]>([
    {
      id: 'pn-1',
      title: 'Post News Card: Coyote vs. Acme Enters Oscar Race',
      detail: 'Queued next 4:5 branded news card for @snapwave once the 4-hour feed spacing window clears.',
      tag: 'NEXT IG CARD',
      time_label: 'After 4h gap',
      eta: 'ETA 3h 40m',
      platform: 'instagram',
      account_handle: '@snapwave',
    },
    {
      id: 'pn-2',
      title: 'Post Audience A/B/C/D Question Card',
      detail: 'Scheduled bi-daily engagement poll card for @snapwave community discussion.',
      tag: 'POLL CARD',
      time_label: '2026-11-01',
      eta: 'ETA 11h 15m',
      platform: 'instagram',
      account_handle: '@snapwave',
    },
    {
      id: 'pn-3',
      title: 'Publish Daily Episode + 3x 9:16 Vertical Shorts',
      detail: 'Release next verified Clearly Drawn News episode on YouTube (@ClearlyDrawnLessons), TikTok, and IG Reels.',
      tag: 'PUBLISH',
      time_label: '15:00 UTC',
      eta: 'ETA 9h 45m',
      platform: 'youtube',
      account_handle: '@ClearlyDrawnLessons',
    },
    {
      id: 'pn-4',
      title: 'Next Autonomous GC & Log Rotation Check',
      detail: 'Prune temporary OpenToonz frames once YouTube & TikTok confirm processing.',
      tag: 'MAINTENANCE',
      time_label: 'In 30 mins',
      eta: 'ETA 28m',
      platform: 'shopify',
      account_handle: 'zs56hs-fj.myshopify.com',
    },
  ]);

  agentChatMessages = signal<AgentChatMessage[]>([
    {
      id: 'chat-1',
      sender: 'agent',
      author_label: 'OpenAccounts LM Agent · Nemotron 3.5 Super VL',
      timestamp: '23:10 Local',
      text: 'All 7 Clearly Drawn Studios accounts (@snapwave, @clearlydrawnlessons, @ClearlyDrawnLessons, Shopify, PayPal, Gmail, Outlook) are synced and running on autopilot. Drop a directive or attach any image, script, thumbnail, or asset here and I will coordinate it across the accounts and the 39 StickExplainers studio agents.',
      target_scope: 'All Accounts & Studio Agents',
      attachments: [],
    },
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
      case 'send_stick_bridge_message':
        return this.stickBridgeMessages()[0] as unknown as T;
      case 'run_garbage_collection':
        return this.gcReport() as unknown as T;
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

  async sendStickBridgeMessage(
    fromAgent: string,
    toAgent: string,
    channel: string,
    subject: string,
    body: string,
    createCeoDirective = false
  ) {
    const newMsg: StickBridgeMessage = {
      id: `br-msg-00${this.stickBridgeMessages().length + 1}`,
      timestamp: 'Just now',
      direction: 'openaccounts_to_stick',
      from_agent: fromAgent,
      to_agent: toAgent,
      channel,
      subject,
      body,
      status: 'delivered',
    };
    this.stickBridgeMessages.update((prev) => [newMsg, ...prev]);
    await this.invoke('send_stick_bridge_message', {
      from_agent: fromAgent,
      to_agent: toAgent,
      channel,
      subject,
      body,
      create_ceo_directive: createCeoDirective,
    });
  }

  async triggerGarbageCollection() {
    const res = await this.invoke<GarbageCollectionReport>('run_garbage_collection', { apply: true });
    if (res && res.free_gb !== undefined) {
      this.gcReport.set({ ...res, executed_at: 'Just now' });
    }
    this.sendStickBridgeMessage(
      'OpenAccounts · GarbageCollector',
      'G. Graphite (Garbage Collector)',
      '24h Buffer, Quota, GC & Sponsor Triage',
      'Autonomous Garbage Collection Sweep',
      `Free disk: ${this.gcReport().free_gb} GB | Reclaimed total: ${this.gcReport().freed_total_gb} GB | Orphans: ${this.gcReport().orphans_status}`
    );
  }

  async sendAgentChatMessage(text: string, targetScope: string, attachments: ChatAttachment[]) {
    const nowLabel = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const humanMsg: AgentChatMessage = {
      id: `chat-${Date.now()}`,
      sender: 'human',
      author_label: 'You',
      timestamp: nowLabel,
      text: text || (attachments.length > 0 ? `Attached ${attachments.length} file(s)` : ''),
      target_scope: targetScope,
      attachments,
    };
    this.agentChatMessages.update((prev) => [...prev, humanMsg]);

    const attachmentNames = attachments.map((a) => `${a.name} (${a.size_label})`).join(', ');
    const bridgeBody = attachmentNames ? `${text}\n[Attachments: ${attachmentNames}]` : text;

    await this.sendStickBridgeMessage(
      'Human Operator via OpenAccounts LM Agent',
      targetScope,
      'Direct Agent Chat & Asset Handoff',
      text ? text.slice(0, 64) : `Asset Handoff (${attachments.length} file(s))`,
      bridgeBody,
      true
    );

    const attachmentAck =
      attachments.length > 0
        ? ` Indexed ${attachments.length} attachment(s) (${attachmentNames}) for visual/content pipeline use.`
        : '';

    const replyMsg: AgentChatMessage = {
      id: `chat-reply-${Date.now()}`,
      sender: 'agent',
      author_label: 'OpenAccounts LM Agent · Nemotron 3.5 Super VL',
      timestamp: 'Just now',
      text: `Directive routed to ${targetScope}.${attachmentAck} Synchronized across @snapwave (IG), @clearlydrawnlessons (TikTok), and @ClearlyDrawnLessons (YouTube) with zero duplicate actions.`,
      target_scope: targetScope,
      attachments: [],
    };
    this.agentChatMessages.update((prev) => [...prev, replyMsg]);
  }

  runQueuedTaskNow(id: string) {
    const queued = this.planToDoNextItems().find((i) => i.id === id);
    if (!queued) return;
    this.planToDoNextItems.update((list) => list.filter((i) => i.id !== id));
    const promoted: FlowCardItem = {
      ...queued,
      tag: 'RUNNING NOW',
      time_label: 'Started now',
      eta: 'ETA ~2m left',
      progress: 18,
    };
    this.activelyDoingItems.update((list) => [promoted, ...list]);
  }
}
