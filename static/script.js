// script.js (place in /static/script.js)
document.addEventListener('DOMContentLoaded', () => {
  const container = document.getElementById('carousel');
  if (!container) {
    return; // No carousel on this page
  }
  const slides = Array.from(container.children);
  const prevBtn = document.getElementById('prevBtn');
  const nextBtn = document.getElementById('nextBtn');
  let index = 0;
  const total = slides.length;

  function updateCarousel() {
    container.style.transform = `translateX(-${index * 100}%)`;
  }

  function showNext() {
    index = (index + 1) % total;
    updateCarousel();
  }

  function showPrev() {
    index = (index - 1 + total) % total;
    updateCarousel();
  }

  // Auto-loop every 4 seconds
  let interval = setInterval(showNext, 4000);

  // Pause on hover
  container.addEventListener('mouseenter', () => clearInterval(interval));
  container.addEventListener('mouseleave', () => {
    interval = setInterval(showNext, 4000);
  });

  nextBtn.addEventListener('click', showNext);
  prevBtn.addEventListener('click', showPrev);
});

document.querySelectorAll('.faq-question').forEach(q => {
  q.addEventListener('click', () => {
    q.parentElement.classList.toggle('open');
  });
});

