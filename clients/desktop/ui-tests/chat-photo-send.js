// Chat photo sending smoke preserves metadata, refusal and Attach recovery assertions.
(() => {
  const taggedPhoto = (original) => {
    const secret = [...new TextEncoder().encode('GPS-SECRET')];
    const plain = [...new Uint8Array(original)];
    const tagged = new File(
      [
        new Uint8Array([
          ...plain.slice(0, 2),
          0xff,
          0xe1,
          0,
          secret.length + 2,
          ...secret,
          ...plain.slice(2),
        ]),
      ],
      'a.jpg',
      { type: 'image/jpeg' },
    );
    return { secret, tagged };
  };
  const checkSentPhoto = async (fixture, secret, tagged) => {
    const { peer, call } = fixture;
    await appBindings.sendChatPhoto(tagged, call);
    const holds = (bytes) =>
      bytes.some((_, at) => secret.every((byte, i) => bytes[at + i] === byte));
    const { body, headers } = fixture.sent;
    if (
      headers['dyapp-peer-id'] !== peer ||
      headers['dyapp-width'] !== '8' ||
      headers['dyapp-height'] !== '8' ||
      !(body instanceof Uint8Array) ||
      body.length !== 8 * 8 * 4
    ) {
      throw new Error(`the photo pixels were not sent: ${headers['dyapp-width']}`);
    }
    if (holds([...body])) throw new Error('the metadata left the page');
  };
  const checkRefusedPhotos = async (fixture, tagged) => {
    const { sends, call } = fixture;
    const { strings } = appApi;
    const status = document.getElementById('chat-compose-status');
    const refused = async (file, key) => {
      await appBindings.sendChatPhoto(file, call);
      if (status.textContent !== strings[key]) throw new Error(`no ${key}: ${status.textContent}`);
    };
    sends.push('full');
    await refused(tagged, 'chats.photoFull');
    sends.push('notConnected');
    await refused(tagged, 'chats.notConnected');
    await refused(new File(['GIF89a'], 'a.gif', { type: 'image/gif' }), 'chats.photoUnsupported');
    await refused(
      new File([new Uint8Array(4)], 'a.png', { type: 'image/png' }),
      'chats.photoUnsupported',
    );
    if (document.getElementById('chat-attach').disabled) throw new Error('Attach stayed off');
    document.getElementById('chat-back').click();
    await appApi.showChats(() => Promise.resolve([]));
  };
  smoke.uiSmokeChatPhotoSend = async (fixture) => {
    if (document.getElementById('chat-attach').hidden) throw new Error('an open chat hid Attach');
    const { secret, tagged } = taggedPhoto(fixture.original);
    await checkSentPhoto(fixture, secret, tagged);
    await checkRefusedPhotos(fixture, tagged);
  };
})();
