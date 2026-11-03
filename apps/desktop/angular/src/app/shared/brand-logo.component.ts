import { Component, computed, input } from '@angular/core';

/** The platform mark for an account or job. Unknown platforms get a neutral tile. */
@Component({
  selector: 'app-brand-logo',
  standalone: true,
  template: `
    <span class="brand-logo" [class]="'brand-logo brand-' + key()" [style.width.px]="size()" [style.height.px]="size()" [title]="key()">
      @switch (key()) {
        @case ('instagram') {
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="5" /><circle cx="12" cy="12" r="4" /><circle cx="17.5" cy="6.5" r="1" fill="currentColor" stroke="none" />
          </svg>
        }
        @case ('tiktok') {
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M16.5 3c.4 2.4 2 4 4.5 4.2v3.2c-1.7 0-3.2-.5-4.5-1.4v6.4A5.8 5.8 0 1 1 10.7 9.6v3.3a2.6 2.6 0 1 0 2.6 2.6V3h3.2Z" />
          </svg>
        }
        @case ('youtube') {
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M22 8.2c-.2-1.4-1-2.3-2.3-2.5C17.6 5.4 12 5.4 12 5.4s-5.6 0-7.7.3C3 5.9 2.2 6.8 2 8.2 1.7 10 1.7 12 1.7 12s0 2 .3 3.8c.2 1.4 1 2.3 2.3 2.5 2.1.3 7.7.3 7.7.3s5.6 0 7.7-.3c1.3-.2 2.1-1.1 2.3-2.5.3-1.8.3-3.8.3-3.8s0-2-.3-3.8ZM10 15V9l5.3 3L10 15Z" />
          </svg>
        }
        @case ('shopify') {
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M15.3 4.6c-.1 0-.3-.1-.5-.1-.2-.5-.5-1.2-1.1-1.8-.9-1-2-1-2.3-1h-.2C10.9 1.3 10.4 1 9.8 1 7.6 1 6.5 3.8 6.1 5.2l-2.2.7C3.2 6.1 3.2 6.1 3.1 6.8L1.5 19.7 13.6 22l6.9-1.5-5.2-15.9Zm-4.6-1.1c-.4.1-.9.3-1.4.5V3.8c0-.6-.1-1.1-.2-1.5.7.1 1.2.8 1.6 1.2ZM9 2.3c.1.4.2 1 .2 1.8v.2L7.3 4.9c.4-1.4 1.1-2.2 1.7-2.6Z" />
          </svg>
        }
        @case ('paypal') {
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M7.1 21h3l.8-5h2.7c3.7 0 6-1.8 6.6-5.4.3-1.8 0-3.1-.9-4C18.4 5.5 16.9 5 14.9 5H9.4L7.1 21Zm4.3-7.8.7-4.6h2.5c1 0 1.7.2 2.1.6.4.4.5 1 .3 1.8-.4 1.6-1.5 2.2-3.3 2.2h-2.3Z" />
          </svg>
        }
        @case ('email') {
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" />
          </svg>
        }
        @default {
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="4" y="4" width="16" height="16" rx="4" /><circle cx="12" cy="12" r="3" fill="currentColor" stroke="none" />
          </svg>
        }
      }
    </span>
  `,
})
export class BrandLogoComponent {
  platform = input<string>('');
  size = input<number>(20);
  key = computed(() => {
    const p = (this.platform() || '').toLowerCase();
    if (p.includes('gmail') || p.includes('outlook') || p === 'email') return 'email';
    if (p.includes('shopify')) return 'shopify';
    if (p.includes('paypal')) return 'paypal';
    return p || 'studio';
  });
}
