export function submittedUsername(input: string): string {
  return input.trim() || '<No Name>';
}

export class UsernameChooser {
  private readonly overlay = document.querySelector(
    '#join-overlay'
  ) as HTMLElement;
  private readonly form = document.querySelector(
    '#join-form'
  ) as HTMLFormElement;
  private readonly input = document.querySelector(
    '#name-input'
  ) as HTMLInputElement;

  constructor(onPlay: (name: string) => void) {
    this.form.addEventListener('submit', event => {
      event.preventDefault();
      onPlay(submittedUsername(this.input.value));
    });
  }

  show(visible: boolean): void {
    this.overlay.classList.toggle('hidden', !visible);
    if (visible) {
      this.input.focus();
    }
  }
}
