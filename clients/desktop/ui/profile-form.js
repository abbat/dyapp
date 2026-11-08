// Profile form elements are built only after app.js validates the identity generation.
window.profileForm = {
  scaleRow(scale, draft) {
    const label = document.createElement('label');
    const select = document.createElement('select');
    select.id = `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`;
    label.htmlFor = select.id;
    label.textContent = window.strings[`profile.field.${scale.key}`];
    const unset = document.createElement('option');
    unset.value = '0';
    unset.textContent = window.strings['profile.unset'];
    select.append(
      unset,
      ...scale.choices.map((choice) => {
        const option = document.createElement('option');
        option.value = String(choice.code);
        option.textContent = window.strings[`profile.value.${scale.key}.${choice.key}`];
        return option;
      }),
    );
    select.value = String(draft[camel(scale.key)] || 0);
    const row = document.createElement('div');
    row.className = 'field';
    row.append(label, select);
    return row;
  },
  countryOptions(countries) {
    const unset = document.createElement('option');
    unset.value = '';
    unset.textContent = window.strings['profile.unset'];
    return [
      unset,
      ...countries
        .map((code) => [code, countryName(code)])
        .sort(([, a], [, b]) => a.localeCompare(b, window.locale))
        .map(([code, name]) => {
          const option = document.createElement('option');
          option.value = code;
          option.textContent = name;
          return option;
        }),
    ];
  },
};
