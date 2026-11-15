// Actual safety UI/core commands need no daemon; this card is a controlled local draft fixture.
smoke.checkOfflineSafetyNumber = async () => {
  const view = await appBindings.invoke('load_profile');
  const found = {
    peerId: appBindings.current.peerId,
    draft: { ...view.draft, photos: [] },
    age: null,
    place: '',
  };
  const card = appBindings.resultCard(found, 'offline-safety-result');
  await openArea('search-area');
  appBindings.searchResults.append(card);
  try {
    card.click();
    await Promise.all([appApi.favoriteShown, appApi.noteShown, appApi.peerPhotosShown]);
    await smoke.checkSafetyNumber();
  } finally {
    appBindings.closeFound();
    card.remove();
  }
};
