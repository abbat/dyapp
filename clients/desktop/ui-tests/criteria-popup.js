// Counterfactual measurements are allowed only when popup geometry and paint stay equivalent.
(() => {
  const bounds = (node) => {
    const rect = node.getBoundingClientRect();
    return [rect.left, rect.top, rect.width, rect.height];
  };
  const visible = (node) =>
    node.getClientRects().length && getComputedStyle(node).visibility === 'visible';
  const stacking = (style) =>
    style.transform !== 'none' ||
    style.filter !== 'none' ||
    style.perspective !== 'none' ||
    style.opacity !== '1' ||
    style.isolation === 'isolate' ||
    style.mixBlendMode !== 'normal' ||
    style.willChange !== 'auto' ||
    /paint|strict|content/.test(style.contain);
  const clipped = (style) =>
    style.clipPath !== 'none' ||
    style.clip !== 'auto' ||
    (style.maskImage && style.maskImage !== 'none');
  const plainAncestors = (menu) => {
    for (let node = menu.parentElement; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (
        stacking(style) ||
        style.zIndex !== 'auto' ||
        ['fixed', 'sticky'].includes(style.position) ||
        style.overflowX !== 'visible' ||
        style.overflowY !== 'visible' ||
        clipped(style)
      )
        return false;
    }
    return true;
  };
  const inViewport = (menu) => {
    const box = menu.getBoundingClientRect();
    return (
      box.left >= 0 &&
      box.top >= 0 &&
      box.right <= document.documentElement.clientWidth &&
      box.bottom <= document.documentElement.clientHeight
    );
  };
  const restoreStyle = (node, style) => {
    if (style === null) node.removeAttribute('style');
    else node.setAttribute('style', style);
  };
  smoke.popupMenus = () =>
    [...document.querySelectorAll('[role="menu"]')].filter(
      (node) => visible(node) && ['absolute', 'fixed'].includes(getComputedStyle(node).position),
    );
  const pin = (menu) => {
    if (!plainAncestors(menu) || !inViewport(menu) || clipped(getComputedStyle(menu))) return null;
    const nodes = [menu, ...menu.querySelectorAll('*')];
    const before = nodes.map(bounds);
    const saved = menu.getAttribute('style');
    const rect = menu.getBoundingClientRect();
    const properties = {
      position: 'fixed',
      left: `${rect.left}px`,
      top: `${rect.top}px`,
      right: 'auto',
      bottom: 'auto',
      width: `${rect.width}px`,
      height: `${rect.height}px`,
      'box-sizing': 'border-box',
      margin: '0',
    };
    let accepted = false;
    try {
      for (const [name, value] of Object.entries(properties))
        menu.style.setProperty(name, value, 'important');
      accepted = nodes.every((node, index) =>
        bounds(node).every((value, axis) => value === before[index][axis]),
      );
      return accepted ? [menu, saved] : null;
    } finally {
      if (!accepted) restoreStyle(menu, saved);
    }
  };
  smoke.withPopupGeometry = (measure) => {
    const position = [scrollX, scrollY];
    const saved = [];
    try {
      for (const menu of smoke.popupMenus()) {
        const record = pin(menu);
        if (record) saved.push(record);
      }
      return measure();
    } finally {
      for (const [node, style] of saved.reverse()) restoreStyle(node, style);
      if (scrollX !== position[0] || scrollY !== position[1])
        window.scrollTo({ left: position[0], top: position[1], behavior: 'instant' });
    }
  };
  const overlap = (left, right) =>
    left.left < right.right &&
    left.right > right.left &&
    left.top < right.bottom &&
    left.bottom > right.top;
  const positionedPseudo = (node) =>
    ['::before', '::after'].some((pseudo) => {
      const style = getComputedStyle(node, pseudo);
      return !['none', 'normal'].includes(style.content) && style.position !== 'static';
    });
  const couldCover = (node, menu, box, layer) => {
    if (node.contains(menu) || menu.contains(node) || !visible(node)) return false;
    const style = getComputedStyle(node);
    if (style.opacity === '0') return false;
    // Pseudo-element paint can extend beyond its host's measurable rectangle.
    if (positionedPseudo(node)) return true;
    if (!overlap(node.getBoundingClientRect(), box)) return false;
    return stacking(style) || (style.zIndex !== 'auto' && Number(style.zIndex) >= layer);
  };
  smoke.popupCoversText = (menu, element) => {
    const box = menu.getBoundingClientRect();
    const range = document.createRange();
    range.selectNodeContents(element);
    return [...range.getClientRects()].some(
      (rect) => rect.width > 0 && rect.height > 0 && overlap(rect, box),
    );
  };
  smoke.popupPaintEligible = (menu) => {
    const style = getComputedStyle(menu);
    const layer = Number(style.zIndex);
    if (
      !plainAncestors(menu) ||
      !inViewport(menu) ||
      !(layer > 0) ||
      stacking(style) ||
      clipped(style) ||
      style.backgroundImage !== 'none' ||
      window.axe.commons.color.getOwnBackgroundColor(style).alpha !== 1
    )
      return false;
    const box = menu.getBoundingClientRect();
    return ![...document.querySelectorAll('body *')].some((node) =>
      couldCover(node, menu, box, layer),
    );
  };
})();
