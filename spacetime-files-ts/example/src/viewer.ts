import type { FileSummary } from './module_bindings/app/types';
import { baseName } from './paths';
import {
  escapeHtml,
  formatFileSize,
  humanError,
  timestampMilliseconds,
} from './presentation';

const element = <T extends HTMLElement = HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

export interface FileViewerServices {
  loadBlob(file: FileSummary): Promise<Blob>;
  download(file: FileSummary): Promise<void>;
  iconHtml(name: 'file' | 'download'): string;
}

export class FileViewer {
  private files: FileSummary[] = [];
  private index = -1;
  private ownedUrl: string | null = null;
  private generation = 0;
  private scale: number | null = null;

  path: string | null = null;

  constructor(private readonly services: FileViewerServices) {}

  isOpen(): boolean {
    return element('lightbox').classList.contains('open');
  }

  currentFile(): FileSummary | undefined {
    return this.path
      ? this.files.find(file => file.path === this.path)
      : undefined;
  }

  async open(path: string, files: FileSummary[]): Promise<void> {
    this.files = files.slice();
    const index = Math.max(
      0,
      this.files.findIndex(file => file.path === path)
    );
    this.scale = null;
    element('lightbox').classList.add('open');
    await this.load(index);
  }

  step(delta: number): void {
    if (this.files.length < 2) return;
    this.scale = null;
    void this.load(
      (this.index + delta + this.files.length) % this.files.length
    );
  }

  close(): void {
    this.generation++;
    element('lightbox').classList.remove('open');
    element('lb-stage').innerHTML = '';
    this.setZoomControls(false);
    this.releaseUrl();
    this.path = null;
  }

  zoom(factor: number): void {
    const image = element('lb-stage').querySelector('img');
    if (!image) return;
    if (this.scale === null) this.scale = image.width / image.naturalWidth || 1;
    this.scale = Math.min(8, Math.max(0.1, this.scale * factor));
    this.applyScale();
  }

  fit(): void {
    this.scale = null;
    this.applyScale();
  }

  fullSize(): void {
    this.scale = 1;
    this.applyScale();
  }

  private releaseUrl(): void {
    if (!this.ownedUrl) return;
    URL.revokeObjectURL(this.ownedUrl);
    this.ownedUrl = null;
  }

  private applyScale = (): void => {
    const image = element('lb-stage').querySelector('img');
    if (!image) return;
    if (this.scale === null) {
      image.classList.add('fit');
      image.style.width = '';
    } else {
      image.classList.remove('fit');
      image.style.width = `${image.naturalWidth * this.scale}px`;
    }
  };

  private setZoomControls(visible: boolean): void {
    for (const id of ['lb-out', 'lb-in', 'lb-fit', 'lb-full']) {
      element(id).style.display = visible ? '' : 'none';
    }
  }

  private async load(index: number): Promise<void> {
    const row = this.files[index];
    if (!row) return;
    const generation = ++this.generation;
    this.index = index;
    this.path = row.path;
    element('lb-title').textContent = baseName(row.path);
    const updatedAtMs = timestampMilliseconds(row.updatedAt);
    element('lb-meta').textContent = [
      row.mimeType || 'file',
      formatFileSize(row.size),
      row.visibility === 'public' ? 'Public' : 'Private',
      updatedAtMs ? new Date(updatedAtMs).toLocaleString() : '',
      this.files.length > 1 ? `${index + 1}/${this.files.length}` : '',
    ]
      .filter(Boolean)
      .join(' | ');
    element('lb-meta').title = row.sha256Hex ? `SHA-256 ${row.sha256Hex}` : '';
    element<HTMLButtonElement>('lb-prev').disabled = this.files.length < 2;
    element<HTMLButtonElement>('lb-next').disabled = this.files.length < 2;
    await this.buildStage(row, generation);
  }

  private async buildStage(
    row: FileSummary,
    generation: number
  ): Promise<void> {
    const stage = element('lb-stage');
    const mime = row.mimeType || '';
    const previewable =
      mime.startsWith('image/') ||
      mime.startsWith('audio/') ||
      mime.startsWith('video/') ||
      mime.startsWith('text/') ||
      mime === 'application/json' ||
      mime === 'application/pdf';
    this.releaseUrl();
    this.setZoomControls(false);
    if (!previewable) {
      stage.innerHTML = `<div class="notice">${this.services.iconHtml('file')}<div>No inline preview for this type.</div><button class="primary" data-vdl>${this.services.iconHtml('download')}Download</button></div>`;
      stage
        .querySelector('[data-vdl]')
        ?.addEventListener('click', () => void this.services.download(row));
      return;
    }

    let blob: Blob;
    try {
      blob = await this.services.loadBlob(row);
    } catch (error) {
      if (generation !== this.generation) return;
      stage.innerHTML = `<div class="notice">${this.services.iconHtml('file')}<div>Could not load this file: ${escapeHtml(humanError(error))}</div></div>`;
      return;
    }
    if (generation !== this.generation) return;
    if (mime.startsWith('text/') || mime === 'application/json') {
      const text = await blob.text();
      if (generation !== this.generation) return;
      stage.innerHTML = `<pre>${escapeHtml(text.slice(0, 20000))}</pre>`;
      return;
    }

    this.ownedUrl = URL.createObjectURL(blob);
    if (mime.startsWith('image/')) {
      stage.innerHTML = `<img class="fit" src="${this.ownedUrl}" alt="${escapeHtml(baseName(row.path))}" />`;
      this.scale = null;
      stage.querySelector('img')!.onload = this.applyScale;
      this.setZoomControls(true);
    } else if (mime.startsWith('audio/')) {
      stage.innerHTML = `<audio controls src="${this.ownedUrl}"></audio>`;
    } else if (mime.startsWith('video/')) {
      stage.innerHTML = `<video controls src="${this.ownedUrl}"></video>`;
    } else {
      stage.innerHTML = `<iframe class="pdf" src="${this.ownedUrl}" title="PDF preview"></iframe>`;
    }
  }
}
