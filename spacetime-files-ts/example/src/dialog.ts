import { escapeHtml } from './utils';

const element = <T extends HTMLElement = HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

export interface DialogOptions {
  okLabel?: string;
  danger?: boolean;
  altLabel?: string | null;
  onAlt?: (() => void | Promise<void>) | null;
}

export class DialogController {
  private onSave: (() => void | Promise<void>) | null = null;

  constructor(private readonly reportError: (error: unknown) => void) {}

  open = (
    title: string,
    bodyHtml: string,
    onSave: (() => void | Promise<void>) | null,
    options: DialogOptions = {}
  ): void => {
    const {
      okLabel = 'Save',
      danger = false,
      altLabel = null,
      onAlt = null,
    } = options;
    element('dialog-title').textContent = title;
    element('dialog-body').innerHTML = bodyHtml;
    const ok = element('dialog-ok');
    ok.textContent = okLabel;
    ok.classList.toggle('danger', danger);
    ok.classList.toggle('primary', !danger);
    const alt = element<HTMLButtonElement>('dialog-alt');
    alt.hidden = !altLabel;
    if (altLabel) {
      alt.textContent = altLabel;
      alt.onclick = () => {
        void (async () => {
          try {
            await onAlt?.();
            this.close();
          } catch (error) {
            this.reportError(error);
          }
        })();
      };
    }
    this.onSave = onSave;
    element('dialog').classList.add('open');
    setTimeout(
      () =>
        element('dialog-body')
          .querySelector<HTMLElement>('input,select,button')
          ?.focus(),
      20
    );
  };

  close = (): void => {
    element('dialog').classList.remove('open');
    this.resetChrome();
    this.onSave = null;
  };

  commit = async (): Promise<void> => {
    if (!this.onSave) return this.close();
    try {
      await this.onSave();
      this.close();
    } catch (error) {
      this.reportError(error);
    }
  };

  confirm(
    title: string,
    message: string,
    onConfirm: () => void | Promise<void>
  ): void {
    this.open(title, `<p>${escapeHtml(message)}</p>`, onConfirm, {
      okLabel: 'Delete',
      danger: true,
    });
  }

  private resetChrome(): void {
    element('dialog-body').className = 'card-body';
    element('dialog-ok').style.display = '';
    element('dialog-cancel').textContent = 'Cancel';
    const alt = element<HTMLButtonElement>('dialog-alt');
    alt.hidden = true;
    alt.onclick = null;
  }
}
