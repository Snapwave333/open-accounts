import { Pipe, PipeTransform } from '@angular/core';
import { timeAgo } from '../services/tauri.service';

@Pipe({ name: 'timeAgo', standalone: true, pure: false })
export class TimeAgoPipe implements PipeTransform {
  transform(value: string | null | undefined): string {
    return timeAgo(value);
  }
}

@Pipe({ name: 'localWhen', standalone: true })
export class LocalWhenPipe implements PipeTransform {
  transform(value: string | null | undefined, mode: 'time' | 'datetime' = 'datetime'): string {
    if (!value) return '—';
    const d = new Date(value);
    if (Number.isNaN(d.getTime())) return value;
    if (mode === 'time') return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    return d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }
}
