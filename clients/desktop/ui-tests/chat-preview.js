(() => {
  // Core supplies the bounded payload; the list only localizes its typed shape.
  const check = (condition, reason) => {
    if (!condition) throw new Error(reason);
  };
  smoke.checkChatPreviews = (build, strings, formatTime) => {
    const peer = '8'.repeat(64);
    const cases = [
      [{ kind: 'empty' }, strings['chats.noMessages']],
      [{ kind: 'deleted' }, strings['chats.deleted']],
      [{ kind: 'text', text: 'Core preview…' }, 'Core preview…'],
      [{ kind: 'photo' }, strings['chats.photo']],
      [
        { kind: 'sticker', emoji: '👩‍💻', packName: 'Core pack' },
        strings['stickers.message'].replace('{emoji}', '👩‍💻').replace('{name}', 'Core pack'),
      ],
      [{ kind: 'profile', peerId: peer }, `${strings['chats.profileCard']} @888888`],
    ];
    for (const [body, expected] of cases) {
      for (const forwarded of [false, true]) {
        const preview = { body, forwarded, sentAtMs: 1_700_000_000_000 };
        const entry = {
          peerId: peer,
          state: 'open',
          // This legacy field deliberately disagrees with the core projection.
          lastMessage: { text: 'Full history must not appear here', sentAtMs: null },
          preview,
          unreadCount: 0,
        };
        let opened = 0;
        const row = build(entry, () => {
          opened += 1;
        });
        const label = row.querySelector('.chat-preview');
        const prefix = forwarded ? `${strings['chats.forwarded']} · ` : '';
        check(
          label?.textContent === prefix + expected && !label.classList.contains('ellipsis'),
          `Chat list lost the core ${body.kind} preview`,
        );
        check(
          row.querySelector('.meta .caption').textContent === formatTime(preview),
          'Chat list used the legacy message timestamp',
        );
        row.click();
        check(
          opened === 1 && entry.lastMessage.text === 'Full history must not appear here',
          'Chat preview changed conversation data or its opening action',
        );
      }
    }
  };
})();
