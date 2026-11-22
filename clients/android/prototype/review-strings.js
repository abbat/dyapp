// Illustrative frozen comparisons, not network profiles or a field-merging implementation.
(() => {
  const fields = (language, rows) => {
    for (const [field, title, local, remote] of rows) {
      STRINGS[language][`review.field.${field}`] = title;
      STRINGS[language][`review.sample.${field}.local`] = local;
      STRINGS[language][`review.sample.${field}.remote`] = remote;
    }
  };
  Object.assign(STRINGS.en, {
    'review.title': 'Review profile fields',
    'review.intro':
      'Choose which value to keep for every differing field. Nothing is chosen for you.',
    'review.local': 'On this phone',
    'review.remote': 'Verified network snapshot',
    'review.authority': 'These choices cannot approve a device or restore a revoked device.',
    'review.noPublish':
      'Saving creates a local draft. Publishing edits and finishing device setup are separate actions.',
    'review.equal':
      'There are no differing editable fields. Confirm this comparison before continuing.',
    'review.loading': 'Loading a fresh comparison…',
    'review.saving': 'Saving the chosen fields…',
    'review.saved': 'The choices were saved on this phone. Profile edits have not been published.',
    'review.uncertain':
      'Saving could not be confirmed. Some changes may already be stored. Load a fresh comparison before saving again.',
    'review.invalid': 'These choices could not be saved. Review the values and choose again.',
    'review.refresh': 'Load a fresh comparison',
    'review.preview': 'Preview chosen values',
    'review.previewTitle': 'Chosen values',
    'review.edit': 'Return to choices',
    'review.save': 'Save chosen fields',
    'review.photoDetails': 'Photo references',
    'review.photoLocal': 'Photo kept on this phone',
    'review.photoRemote': 'Photo in the verified network snapshot',
  });
  Object.assign(STRINGS.ru, {
    'review.title': 'Проверьте поля анкеты',
    'review.intro':
      'Выберите значение для каждого различающегося поля. Ни один вариант не выбран заранее.',
    'review.local': 'На этом телефоне',
    'review.remote': 'Проверенный снимок из сети',
    'review.authority':
      'Этим выбором нельзя подтвердить устройство или вернуть доступ отозванному устройству.',
    'review.noPublish':
      'Сохранится только местный черновик. Публикация правок и завершение настройки устройства выполняются отдельно.',
    'review.equal': 'Различающихся полей нет. Подтвердите это сравнение перед продолжением.',
    'review.loading': 'Загрузка нового сравнения…',
    'review.saving': 'Сохранение выбранных полей…',
    'review.saved': 'Выбранные значения сохранены на этом телефоне. Правки анкеты не опубликованы.',
    'review.uncertain':
      'Сохранение не подтверждено. Часть изменений уже могла сохраниться. Загрузите новое сравнение перед повторным сохранением.',
    'review.invalid': 'Не удалось сохранить этот выбор. Проверьте значения и выберите снова.',
    'review.refresh': 'Загрузить новое сравнение',
    'review.preview': 'Показать выбранные значения',
    'review.previewTitle': 'Выбранные значения',
    'review.edit': 'Вернуться к выбору',
    'review.save': 'Сохранить выбранные поля',
    'review.photoDetails': 'Ссылки на фотографии',
    'review.photoLocal': 'Фотография на этом телефоне',
    'review.photoRemote': 'Фотография из проверенного сетевого снимка',
  });
  fields('en', [
    ['name', 'Public name', 'Alex', 'Alexander'],
    ['location', 'Country and place', 'Germany\nBerlin', 'France\nParis'],
    ['income', 'Income bounds', '1099511627776 – 2199023255552', 'Not published – 1099511627776'],
    ['has_kids', 'Has children', 'No', 'Yes'],
    ['wants_more_kids', 'Wants more children', 'Not published', 'No'],
    ['interests', 'Interests', 'Long walks\nPhotography\nLearning languages', 'Cycling\nCooking'],
    ['photos', 'Photos', 'Two photos in this order', 'One photo'],
    ['education', 'Education level', 'Value 999', 'Value 60'],
    ['profession', 'Profession', 'Software engineer', 'Researcher'],
    [
      'company',
      'Company',
      'An intentionally long company name that must wrap completely',
      'A different company',
    ],
    ['industry', 'Industry', 'Technology', 'Education'],
    ['career_stage', 'Career stage', 'Experienced specialist', 'Starting a new career'],
    [
      'work_life_balance',
      'Work–life balance',
      'Time for family and friends',
      'A flexible schedule',
    ],
    [
      'birth',
      'Birth date',
      'Year: 1994\nMonth: not published\nDay: not published',
      'Year: 1995\nMonth: 12\nDay: 31',
    ],
    ['gender', 'Gender', 'Man', 'Other'],
    ['orientation', 'Orientation', 'Heterosexual', 'Bisexual'],
    ['relationship_goal', 'Relationship goal', 'Serious relationship', 'Not sure yet'],
    ['role_preference', 'Role preference', 'Equal', 'Breadwinner'],
    ['career_ambition', 'Career ambition', 'Moderate', 'High'],
    ['financial_philosophy', 'Financial philosophy', 'Balanced', 'Saver'],
    ['employment_status', 'Employment status', 'Full time', 'Self-employed / freelance'],
    ['fitness_level', 'Fitness level', 'Moderate', 'Active'],
    ['zodiac', 'Zodiac sign', 'Not published', 'Aquarius'],
    ['height', 'Height', '178 cm', '179 cm'],
    [
      'about',
      'About you',
      'I enjoy finding new places, quiet conversations and time outdoors. This complete local text remains available for comparison.',
      'I am learning something new and looking for people to share it with. The verified text is shown in full too.',
    ],
    ['smoking', 'Smoking', 'No', 'Sometimes'],
    ['drinking', 'Drinking', 'Sometimes', 'No'],
    ['tattoos', 'Tattoos', 'None', 'Hidden'],
    ['marital_status', 'Marital status', 'Single', 'Divorced'],
    ['pets', 'Pets', 'Yes', 'No'],
    [
      'seeking',
      'Looking for',
      'Genders: woman, other\nAge from: 18\nAge to: 40\nNot looking: no',
      'Genders: woman\nAge from: 25\nAge to: not published\nNot looking: no',
    ],
  ]);
  fields('ru', [
    ['name', 'Публичное имя', 'Алексей', 'Александр'],
    ['location', 'Страна и населённый пункт', 'Германия\nБерлин', 'Франция\nПариж'],
    [
      'income',
      'Границы дохода',
      '1099511627776 – 2199023255552',
      'Не опубликовано – 1099511627776',
    ],
    ['has_kids', 'Есть дети', 'Нет', 'Да'],
    ['wants_more_kids', 'Хочет ещё детей', 'Не опубликовано', 'Нет'],
    [
      'interests',
      'Интересы',
      'Долгие прогулки\nФотография\nИзучение языков',
      'Велосипед\nКулинария',
    ],
    ['photos', 'Фотографии', 'Две фотографии в этом порядке', 'Одна фотография'],
    ['education', 'Уровень образования', 'Значение 999', 'Значение 60'],
    ['profession', 'Профессия', 'Разработчик программного обеспечения', 'Исследователь'],
    [
      'company',
      'Компания',
      'Намеренно длинное название компании, которое должно полностью переноситься на следующие строки',
      'Другая компания',
    ],
    ['industry', 'Отрасль', 'Технологии', 'Образование'],
    ['career_stage', 'Этап карьеры', 'Опытный специалист', 'Начало новой карьеры'],
    [
      'work_life_balance',
      'Баланс работы и личной жизни',
      'Время для семьи и друзей',
      'Гибкое расписание',
    ],
    [
      'birth',
      'Дата рождения',
      'Год: 1994\nМесяц: не опубликовано\nДень: не опубликовано',
      'Год: 1995\nМесяц: 12\nДень: 31',
    ],
    ['gender', 'Пол', 'Мужчина', 'Другое'],
    ['orientation', 'Ориентация', 'Гетеросексуальность', 'Бисексуальность'],
    ['relationship_goal', 'Цель знакомства', 'Серьёзные отношения', 'Пока не знаю'],
    ['role_preference', 'Распределение ролей', 'Наравне', 'Основной доход'],
    ['career_ambition', 'Карьерные амбиции', 'Умеренные', 'Высокие'],
    ['financial_philosophy', 'Отношение к деньгам', 'Сбалансированное', 'Накопления'],
    ['employment_status', 'Занятость', 'Полный день', 'Самозанятость / фриланс'],
    ['fitness_level', 'Физическая активность', 'Умеренная', 'Высокая'],
    ['zodiac', 'Знак зодиака', 'Не опубликовано', 'Водолей'],
    ['height', 'Рост', '178 см', '179 см'],
    [
      'about',
      'О себе',
      'Мне нравятся новые места, спокойные разговоры и время на природе. Этот местный текст целиком доступен для сравнения.',
      'Я учусь новому и ищу людей, с которыми можно это разделить. Проверенный сетевой текст тоже показан полностью.',
    ],
    ['smoking', 'Курение', 'Нет', 'Иногда'],
    ['drinking', 'Алкоголь', 'Иногда', 'Нет'],
    ['tattoos', 'Татуировки', 'Нет', 'Скрытые'],
    ['marital_status', 'Семейное положение', 'Не в браке', 'В разводе'],
    ['pets', 'Животные', 'Да', 'Нет'],
    [
      'seeking',
      'Кого ищете',
      'Пол: женщина, другое\nВозраст от: 18\nВозраст до: 40\nНе ищу: нет',
      'Пол: женщина\nВозраст от: 25\nВозраст до: не опубликовано\nНе ищу: нет',
    ],
  ]);
})();
