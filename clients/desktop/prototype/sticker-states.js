// Local geometry stand-in only: no downloaded or encoded peer media enters this prototype.
(() => {
  const state = location.hash.slice(1);
  const text = (key) => window.strings[key];
  const demo = (canvas) => {
    const ctx = canvas.getContext('2d');
    const scale = canvas.width / 96;
    ctx.scale(scale, scale);
    ctx.fillStyle = '#f0b85b';
    ctx.beginPath();
    ctx.arc(48, 48, 36, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = '#342814';
    ctx.beginPath();
    ctx.arc(34, 40, 4, 0, Math.PI * 2);
    ctx.arc(62, 40, 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = '#342814';
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.arc(48, 48, 18, 0.2, Math.PI - 0.2);
    ctx.stroke();
  };
  const grid = document.getElementById('sticker-picker-grid');
  for (const emoji of ['😺', '🐱', '😸', '😹', '😻', '😼', '😽', '🙀']) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.setAttribute('aria-label', text('stickerPicker.label').replace('😺', emoji));
    button.disabled = state === 'sticker-picker-sending';
    const canvas = document.createElement('canvas');
    canvas.width = 96;
    canvas.height = 96;
    canvas.setAttribute('aria-hidden', 'true');
    button.append(canvas);
    grid.append(button);
    demo(canvas);
  }
  for (const canvas of document.querySelectorAll('[data-demo-sticker]')) demo(canvas);
  if (state.startsWith('sticker-picker-')) {
    document.getElementById('sticker-picker-cancel').focus();
    const recent = state === 'sticker-picker-recent';
    document.getElementById('sticker-picker-recent').setAttribute('aria-pressed', String(recent));
    document.getElementById('sticker-picker-pack').setAttribute('aria-pressed', String(!recent));
    for (const button of document.querySelectorAll('.sticker-choices button')) {
      button.disabled = state === 'sticker-picker-loading' || state === 'sticker-picker-sending';
    }
  }
  if (state.startsWith('chat-sticker-')) {
    const thread = document.getElementById('chat-thread');
    for (const item of thread.children) {
      if (item.id !== 'sticker-message' && !item.classList.contains('day')) item.hidden = true;
    }
  }
  document.getElementById('sticker-picker-cancel').addEventListener('click', () => {
    location.hash = 'chat';
  });
  document.getElementById('sticker-picker-manage').addEventListener('click', () => {
    location.hash = 'stickers-ready';
  });
})();
