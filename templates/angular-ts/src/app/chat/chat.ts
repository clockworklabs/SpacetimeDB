import { CommonModule } from '@angular/common';
import { Component, computed, effect, ElementRef, ViewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { injectReducer, injectTable } from 'spacetimedb/angular';
import { reducers, tables } from '../../module_bindings';

@Component({
  selector: 'app-chat',
  imports: [CommonModule, FormsModule],
  templateUrl: './chat.html',
})
export class Chat {
  onlineUsers = injectTable(tables.onlineUsers);
  onlineUsersCount = computed(() => this.onlineUsers().rows.length);
  messages = injectTable(tables.messages);

  private sendMessage = injectReducer(reducers.sendMessage);

  @ViewChild('messagesContainer') messagesContainer?: ElementRef<HTMLDivElement>;

  protected messageContent = '';

  constructor() {
    effect(() => {
      this.messages(); // Track messages changes
      queueMicrotask(() => this.scrollToBottom());
    });
  }

  protected onSendMessage() {
    const content = this.messageContent.trim();
    if (content === '') {
      return;
    }

    this.sendMessage({ content });
    this.messageContent = '';
  }

  private scrollToBottom() {
    if (this.messagesContainer) {
      const container = this.messagesContainer.nativeElement;
      console.log('Scrolling to bottom of messages container');
      container.scrollTop = container.scrollHeight;
    }
  }
}
