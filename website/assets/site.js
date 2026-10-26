// Fade sections in, switch code tabs, open the mobile menu.
document.documentElement.classList.add("js");
(() => {
  const els = document.querySelectorAll('.reveal');
  if (!('IntersectionObserver' in window)) { els.forEach(e => e.classList.add('in')); return; }
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
  }, { rootMargin: '0px 0px -8% 0px', threshold: 0.08 });
  els.forEach((e, i) => {
    if (e.parentElement && e.parentElement.closest('.probe')) e.style.transitionDelay = (i % 8) * 70 + 'ms';
    io.observe(e);
  });
})();
document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('.tab').forEach(tab => tab.addEventListener('click', () => {
    const box = tab.closest('.code');
    box.querySelectorAll('.tab').forEach(t => t.setAttribute('aria-selected', t === tab ? 'true' : 'false'));
    box.querySelectorAll('.pane').forEach(p => { p.hidden = p.dataset.pane !== tab.dataset.tab; });
  }));
  const nav = document.querySelector('.nav'), burger = document.querySelector('.burger');
  burger && burger.addEventListener('click', () => {
    const open = nav.classList.toggle('open');
    burger.setAttribute('aria-expanded', open ? 'true' : 'false');
  });
  nav && nav.querySelectorAll('.links a').forEach(a => a.addEventListener('click', () => nav.classList.remove('open')));
});
