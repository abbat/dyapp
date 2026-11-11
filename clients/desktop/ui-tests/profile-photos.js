// Profile photo editor smoke phases, preserving draft actions and focus assertions.
(() => {
  const createFixture = async () => {
    const source = document.createElement('canvas');
    source.width = 4;
    source.height = 4;
    source.getContext('2d').fillRect(0, 0, 4, 4);
    const jpeg = await new Promise((resolve) => {
      source.toBlob(resolve, 'image/jpeg');
    });
    const file = new File([jpeg], 'photo.jpg', { type: 'image/jpeg' });
    const bytes = await jpeg.arrayBuffer();
    const hashes = [];
    const commands = [];
    const fixture = { refusal: null };
    const preparation = {
      photo_picked_byte_limit: () => Promise.resolve(64 << 20),
      photo_decode_plan: ({ mime, width, height }) =>
        mime === 'image/jpeg' ? Promise.resolve({ width, height }) : Promise.reject('unsupported'),
    };
    const store = (command, args) => {
      commands.push(command);
      if (command === 'profile_photo') return Promise.resolve(bytes);
      if (fixture.refusal) return Promise.reject(fixture.refusal);
      if (command === 'add_profile_photo') hashes.push([hashes.length + 1]);
      if (command === 'remove_profile_photo') hashes.splice(args.index, 1);
      if (command === 'move_profile_photo')
        hashes.splice(args.to, 0, ...hashes.splice(args.from, 1));
      return Promise.resolve({ draft: { photos: [...hashes] } });
    };
    const call = (command, args) =>
      Object.hasOwn(preparation, command) ? preparation[command](args) : store(command, args);
    const { strings } = appApi;
    const status = byId('profile-status');
    const add = byId('profile-photo-add');
    const grid = byId('profile-photos');
    return Object.assign(fixture, { file, hashes, commands, call, strings, status, add, grid });
  };
  const checkInvalidPhotos = async (fixture) => {
    const { call, grid, add, status, strings, commands, file } = fixture;
    await openArea('profile-area');
    appApi.showProfilePhotos([], call);
    if (grid.children.length || add.disabled || !byId('profile-photo-limit').hidden) {
      throw new Error('an empty editor did not offer a photo');
    }

    await appBindings.addProfilePhoto(new File(['text'], 'a.txt', { type: 'text/plain' }), call);
    if (status.textContent !== strings['profile.photoUnsupported'] || commands.length) {
      throw new Error('a wrong file type was not named or reached the core');
    }
    fixture.refusal = 'photoRefused';
    await appBindings.addProfilePhoto(file, call);
    if (status.textContent !== strings['profile.photoRefused'] || grid.children.length) {
      throw new Error('a refused photo was not named or changed the grid');
    }
    fixture.refusal = null;
  };
  const checkPhotoLimit = async (fixture) => {
    const { file, call, grid, add, status, strings } = fixture;
    for (let added = 0; added < 6; added += 1) await appBindings.addProfilePhoto(file, call);
    await Promise.all([...grid.children].map((tile) => tile.ready));
    if (
      grid.children.length !== 6 ||
      !add.disabled ||
      byId('profile-photo-limit').hidden ||
      !byId('profile-photo-hint').hidden
    ) {
      throw new Error('six photos did not disable Add photo with the reason');
    }
    if (status.textContent !== strings['profile.draft'])
      throw new Error('a photo was not a draft change');
    if (grid.children[0].querySelector('canvas').hidden)
      throw new Error('a stored photo was not drawn');
    if (grid.children[0].querySelector('.left') || grid.children[5].querySelector('.right')) {
      throw new Error('a photo offered a move past the end');
    }
    await smoke.shot('profile-photos');
  };
  const checkPhotoReorderAndRemoval = async (fixture) => {
    const { grid, hashes, add } = fixture;
    grid.children[0].querySelector('.right').click();
    await appApi.profilePhotoChanged;
    if (String(hashes.slice(0, 2)) !== '2,1')
      throw new Error('Move right did not reorder the draft');
    if (!grid.children[1].contains(document.activeElement))
      throw new Error('the move lost the focus');
    const dialog = byId('photo-remove-dialog');
    grid.children[0].querySelector('.remove').click();
    if (!dialog.open || document.activeElement !== byId('photo-remove-cancel')) {
      throw new Error('removal did not ask first with Cancel focused');
    }
    await smoke.shot('photo-remove-dialog');
    byId('photo-remove-cancel').click();
    if (dialog.open || hashes.length !== 6) throw new Error('Cancel removed a photo');
    grid.children[0].querySelector('.remove').click();
    byId('photo-remove-confirm').click();
    await appApi.profilePhotoChanged;
    if (hashes.length !== 5 || add.disabled || document.activeElement !== add) {
      throw new Error('removal did not free a place and move the focus to Add photo');
    }
    // Enter busy state before pending avatar draws get another microtask.
    return checkPhotoBusy(fixture);
  };
  const checkPhotoBusy = async (fixture) => {
    const { commands, file, call, add } = fixture;
    appBindings.setProfileBusy(true);
    if (!add.disabled) throw new Error('Add photo stayed enabled while busy');
    const before = commands.length;
    await appBindings.addProfilePhoto(file, call);
    if (commands.length !== before) throw new Error('a photo changed while the form was busy');
    await checkOwnAvatarPaint(fixture);
    appBindings.setProfileBusy(false);
    if (add.disabled) throw new Error('Add photo stayed disabled after the form was free');
  };
  const checkOwnAvatarPaint = async ({ call, commands }) => {
    const avatar = document.querySelector('#sidebar-identity .avatar');
    await appApi.ownAvatarsShown;
    const letters = avatar.querySelector('[data-avatar-letters]');
    if (!avatar.querySelector('canvas') || !letters?.hidden || !letters.textContent) {
      throw new Error('the identity photo did not hide its letter fallback');
    }
    if (commands.some((name) => name.includes('peer') || name.includes('fetch'))) {
      throw new Error('the own photo touched a peer or node command');
    }
    appApi.showProfilePhotos([], call);
    await appApi.ownAvatarsShown;
    if (avatar.querySelector('canvas') || letters.hidden) {
      throw new Error('an identity without photos did not restore its letters');
    }
    appApi.showProfilePhotos([[1]], () => Promise.resolve(new ArrayBuffer(0)));
    await appApi.ownAvatarsShown;
    if (avatar.querySelector('canvas') || letters.hidden) {
      throw new Error('an undecodable identity photo did not restore its letters');
    }
    appApi.showProfilePhotos([], call);
    await appApi.ownAvatarsShown;
  };
  const checkPhotoPublication = async (fixture) => {
    const { status, strings } = fixture;
    if (
      !appBindings
        .publicFields({
          name: '',
          photos: [[1], [2]],
          birthYear: null,
          birthMonth: null,
          birthDay: null,
        })
        .includes(`${strings['profile.photos']}: 2`)
    ) {
      throw new Error('the publish dialog did not count the photos');
    }
    appBindings.profileFailure('full');
    if (status.textContent !== strings['profile.full'])
      throw new Error('a full node was not named');
    await appBindings.showProfile();
    const kept = document.getElementById('birth-year').value;
    document.getElementById('birth-year').value = '1971';
    await appBindings.profileFailure('changed');
    if (
      status.textContent !== strings['profile.changed'] ||
      document.getElementById('birth-year').value !== kept
    ) {
      throw new Error('a profile from another device was not shown in place of the edit');
    }
  };
  smoke.uiSmokeProfilePhotos = async () => {
    const fixture = await createFixture();
    await checkInvalidPhotos(fixture);
    await checkPhotoLimit(fixture);
    await checkPhotoReorderAndRemoval(fixture);
    await checkPhotoPublication(fixture);
  };
})();
