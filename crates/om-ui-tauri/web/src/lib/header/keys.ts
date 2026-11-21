/** True when a key press belongs to a text field (or a menu), so `r` is a letter, not Record. */
export function takesTyping(target: { tagName?: string; isContentEditable?: boolean } | null): boolean {
  if (!target) return false;
  if (target.isContentEditable) return true;
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName ?? '');
}

/** True for the bare `r` key: no modifier, and not an auto-repeat. */
export function isRecordKey(event: Pick<KeyboardEvent, 'key' | 'repeat' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>): boolean {
  return (
    event.key.toLowerCase() === 'r' && !event.repeat && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey
  );
}
