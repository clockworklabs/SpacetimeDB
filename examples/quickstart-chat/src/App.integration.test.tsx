// src/App.integration.test.tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';

describe('App Integration Test', () => {
  it('connects to the DB, allows name change and message sending', async () => {
    render(<App />);

    // Initially, we should see "Connecting..."
    expect(screen.getByText(/Connecting.../i)).toBeInTheDocument();

    // Wait until "Connecting..." is gone (meaning we've connected)
    // This might require the actual DB to accept the connection
    await waitFor(
      () =>
        expect(screen.queryByText(/Connecting.../i)).not.toBeInTheDocument(),
      { timeout: 10000 }
    );

    // The profile section should show the default name or truncated identity
    // For example, you can check if the text is rendered.
    // If your default identity is something like 'abcdef12' or 'Unknown'
    // we do a generic check:
    expect(
      screen.getByRole('heading', { name: /profile/i })
    ).toBeInTheDocument();

    // Let's change the user's name
    const editNameButton = screen.getByText(/Edit Name/i);
    await userEvent.click(editNameButton);

    const nameInput = screen.getByRole('textbox', { name: /name input/i });
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, 'TestUser');
    const submitNameButton = screen.getByRole('button', { name: /submit/i });
    await userEvent.click(submitNameButton);

    // If your DB or UI updates instantly, we can check that the new name shows up
    await waitFor(
      () => {
        expect(screen.getByText('TestUser')).toBeInTheDocument();
      },
      { timeout: 10000 }
    );

    // Now let's send a message
    const textarea = screen.getByRole('textbox', { name: /message input/i });
    await userEvent.type(textarea, 'Hello from GH Actions!');

    const sendButton = screen.getByRole('button', { name: /send/i });
    await userEvent.click(sendButton);

    // Wait for message to appear in the UI
    await waitFor(
      () => {
        expect(screen.getByText('Hello from GH Actions!')).toBeInTheDocument();
      },
      { timeout: 10000 }
    );
  });
});
