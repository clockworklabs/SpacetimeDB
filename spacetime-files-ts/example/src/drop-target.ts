import { collectDropped } from './uploads';

export interface FolderDropServices {
  currentPath(): string;
  endFileDrag(): void;
  upload(dataTransfer: DataTransfer, folderPath: string): Promise<void>;
  move(paths: string[], folderPath: string): Promise<void>;
}

export function wireFolderDropTarget(
  element: HTMLElement,
  folderPath: string,
  services: FolderDropServices
): void {
  const dropPath = document.getElementById('drop-path')!;
  element.addEventListener('dragover', event => {
    const types = [...event.dataTransfer!.types];
    if (!types.includes('application/x-vault-path') && !types.includes('Files'))
      return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer!.dropEffect = types.includes('Files') ? 'copy' : 'move';
    element.classList.add('drag-over');
    if (types.includes('Files')) dropPath.textContent = folderPath;
  });
  element.addEventListener('dragleave', () => {
    element.classList.remove('drag-over');
    dropPath.textContent = services.currentPath();
  });
  element.addEventListener('drop', event => {
    void (async () => {
      const types = [...event.dataTransfer!.types];
      if (
        !types.includes('application/x-vault-path') &&
        !types.includes('Files')
      )
        return;
      event.preventDefault();
      event.stopPropagation();
      element.classList.remove('drag-over');
      services.endFileDrag();
      if (types.includes('Files')) {
        await services.upload(event.dataTransfer!, folderPath);
        return;
      }
      let paths: string[] = [];
      try {
        paths = JSON.parse(
          event.dataTransfer!.getData('application/x-vault-path')
        ) as string[];
      } catch {
        // Ignore malformed drag data.
      }
      await services.move(paths, folderPath);
    })();
  });
}

export async function uploadDropped(
  dataTransfer: DataTransfer,
  folderPath: string,
  upload: (
    entries: Awaited<ReturnType<typeof collectDropped>>,
    path: string
  ) => Promise<void>
): Promise<void> {
  await upload(await collectDropped(dataTransfer), folderPath);
}
