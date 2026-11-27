// Owner-reviewed linked-device recovery with explicit callback boundaries.
(() => {
  const get = (id) => document.getElementById(id);
  const check = (ok, reason) => {
    if (!ok) throw new Error(reason);
  };
  const deferred = () => {
    let resolve;
    let reject;
    const promise = new Promise((done, fail) => {
      resolve = done;
      reject = fail;
    });
    return { promise, resolve, reject };
  };
  const profile = (name, photo = 1) => ({
    version: '3',
    fields: {
      name,
      gender: 1,
      orientation: 2,
      relationshipGoal: 1,
      rolePreference: 1,
      careerAmbition: 1,
      financialPhilosophy: 1,
      employmentStatus: 1,
      fitnessLevel: 1,
      zodiac: 1,
      birthYear: 1990,
      birthMonth: 4,
      birthDay: 3,
      height: 180,
      about: `${name} about`,
      interests: [`${name} interest`],
      smoking: 1,
      drinking: 1,
      tattoos: 1,
      maritalStatus: 1,
      pets: 1,
      country: 'DE',
      place: '42',
      seekingGenders: [1, 2],
      seekingAgeFrom: 20,
      seekingAgeTo: 99,
      notLooking: false,
      photos: [Array(32).fill(photo)],
    },
    incomeFrom: '10',
    incomeTo: '100',
    hasKids: false,
    wantsMoreKids: true,
    interests: [`${name} interest`],
    photos: [{ thumbnail: Array(32).fill(photo), full: [Array(32).fill(photo + 1)] }],
    educationLevel: 2,
    profession: `${name} profession`,
    company: `${name} company`,
    industry: `${name} industry`,
    careerStage: `${name} stage`,
    workLifeBalance: `${name} balance`,
  });
  const snapshot = (reviewId, differences = ['name', 'photos']) => ({
    reviewId,
    local: profile('Local', 1),
    remote: profile('Remote', 3),
    differences,
    hidden: false,
  });
  const fixture = () => {
    const calls = [];
    const handlers = {};
    const call = (command, args) => {
      calls.push({ command, args });
      if (handlers[command]) return handlers[command](args);
      if (command === 'preview_link_review') return Promise.resolve(profile('Preview', 1));
      if (command === 'confirm_link_review') {
        const current = snapshot(args.reviewId);
        return Promise.resolve({
          published: current.remote,
          draft: current.local,
          hidden: false,
          pending: true,
        });
      }
      if (command === 'resume_link_review') return Promise.resolve(true);
      if (command === 'link_review_photo') {
        return Promise.resolve({ width: 1, height: 1, pixels: [1, 2, 3, 255] });
      }
      return Promise.resolve(null);
    };
    return { calls, handlers, call };
  };
  const open = async (test, id = '1', differences) => {
    test.handlers.prepare_link_review = () => Promise.resolve(snapshot(id, differences));
    await appApi.linkReview.open(test.call);
    check(get('link-review-dialog').open, 'Review did not open');
  };
  const choose = (field, source) => {
    const input = get(`link-review-${field}-${source}`);
    input.checked = true;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const close = () => appApi.linkReview.close();

  const checkChoicePreviewAndPhotos = async (test, shot = smoke.shot) => {
    await open(test);
    check(get('link-review-save').disabled, 'Save admitted an incomplete selection');
    await shot('link-review-choices');
    choose('name', 'local');
    choose('photos', 'remote');
    check(!get('link-review-preview').disabled, 'Complete choices did not enable preview');
    test.handlers.link_review_photo = ({ source }) =>
      source === 'remote'
        ? Promise.reject('photoUnavailable')
        : Promise.resolve({
            width: 1,
            height: 1,
            pixels: [1, 2, 3, 255],
          });
    await open(test, '2');
    choose('name', 'local');
    choose('photos', 'remote');
    await appApi.linkReview.photoDone;
    check(
      get('link-review-photos-remote').checked && !get('link-review-photo-retry-remote-0').hidden,
      'Photo failure discarded the source choice or retry',
    );
    await shot('link-review-photo-error');
    test.handlers.link_review_photo = () =>
      Promise.resolve({ width: 1, height: 1, pixels: [1, 2, 3, 255] });
    get('link-review-photo-retry-remote-0').click();
    await appApi.linkReview.photoDone;
    check(get('link-review-photo-retry-remote-0').hidden, 'Successful photo retry kept failure');
    get('link-review-preview').click();
    await appApi.linkReview.previewDone;
    check(
      !get('link-review-preview-rows').hidden &&
        get('link-review-preview-rows')
          .querySelector('[data-field="name"]')
          ?.textContent.endsWith('\nPreview'),
      'Native preview was not rendered',
    );
    await shot('link-review-preview');
    get('link-review-edit').click();
    check(get('link-review-preview-rows').hidden, 'Edit retained the preview');
  };

  const checkSaveAndResume = async (test, shot = smoke.shot) => {
    await open(test, '3');
    choose('name', 'remote');
    choose('photos', 'local');
    get('link-review-save').click();
    await appApi.linkReview.confirmDone;
    check(!get('link-review-resume').hidden, 'Saved review did not offer separate completion');
    await shot('link-review-saved');
    test.handlers.resume_link_review = () => Promise.reject('notConnected');
    get('link-review-resume').click();
    await appApi.linkReview.resumeDone;
    check(
      get('link-review-resume').disabled &&
        get('link-review-status').textContent === appApi.strings['linkReview.uncertain'],
      'Uncertain resume offered a blind retry',
    );
    await open(test, '4');
    choose('name', 'remote');
    choose('photos', 'local');
    get('link-review-save').click();
    await appApi.linkReview.confirmDone;
    test.handlers.resume_link_review = () => Promise.resolve(true);
    get('link-review-resume').click();
    await appApi.linkReview.resumeDone;
    check(
      get('link-review-resume').disabled &&
        get('link-review-status').textContent === appApi.strings['linkReview.resumed'],
      'Successful resume did not complete the presentation',
    );
    await open(test, 'policy');
    choose('name', 'remote');
    choose('photos', 'local');
    test.handlers.confirm_link_review = () => Promise.reject('deviceNotListed');
    get('link-review-save').click();
    await appApi.linkReview.confirmDone;
    check(
      get('link-review-status').textContent === appApi.strings['linkReview.deviceNotListed'],
      'Policy refusal was hidden by an uncertain backend outcome',
    );
    test.handlers.confirm_link_review = ({ reviewId }) => {
      const current = snapshot(reviewId);
      return Promise.resolve({
        published: current.remote,
        draft: current.local,
        hidden: false,
        pending: true,
      });
    };
  };

  const checkStaleResponses = async (test) => {
    const first = deferred();
    test.handlers.prepare_link_review = () => first.promise;
    const opening = appApi.linkReview.open(test.call);
    close();
    await open(test, 'current');
    first.resolve(snapshot('stale'));
    await opening;
    check(
      get('link-review-name-local-value').textContent === 'Local' &&
        !test.calls.some(
          ({ command, args }) => command === 'confirm_link_review' && args.reviewId === 'stale',
        ),
      'Stale prepare replaced the current review',
    );
    choose('name', 'local');
    choose('photos', 'remote');
    const saving = deferred();
    test.handlers.confirm_link_review = () => saving.promise;
    get('link-review-save').click();
    close();
    await open(test, 'successor');
    saving.resolve({
      published: profile('Remote'),
      draft: profile('Local'),
      hidden: false,
      pending: true,
    });
    await appApi.linkReview.confirmDone;
    check(get('link-review-resume').hidden, 'Stale confirmation completed a successor');
    close();
  };

  const checkIdentityAndEmptyReview = async (test) => {
    const pending = deferred();
    test.handlers.prepare_link_review = () => pending.promise;
    const opening = appApi.linkReview.open(test.call);
    document.dispatchEvent(new Event('dyapp:identity-change'));
    pending.resolve(snapshot('old-identity'));
    await opening;
    check(!get('link-review-dialog').open, 'Identity change retained the review');
    await open(test, 'empty', []);
    get('link-review-preview').click();
    await appApi.linkReview.previewDone;
    get('link-review-save').click();
    await appApi.linkReview.confirmDone;
    const confirmation = test.calls.find(
      ({ command, args }) => command === 'confirm_link_review' && args.reviewId === 'empty',
    );
    check(confirmation.args.selected.length === 0, 'Equal review submitted unexpected choices');
    close();
  };

  smoke.checkLinkReviewCapture = (theme, scale) =>
    smoke.withCaptureAppearance(theme, scale, async () => {
      const test = fixture();
      const text = scale === '100' ? 'normal' : 'large';
      const shot = (name) => smoke.waitForSignal(name, smoke.shot(`${name}-${theme}-${text}`));
      try {
        await checkChoicePreviewAndPhotos(test, shot);
        await checkSaveAndResume(test, shot);
      } finally {
        close();
      }
    });

  smoke.checkLinkReview = async () => {
    const test = fixture();
    try {
      await checkChoicePreviewAndPhotos(test);
      await checkSaveAndResume(test);
      await checkStaleResponses(test);
      await checkIdentityAndEmptyReview(test);
    } finally {
      close();
    }
  };
})();
