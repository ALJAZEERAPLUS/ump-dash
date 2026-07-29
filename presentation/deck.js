const slides = [...document.querySelectorAll(".slide")];
const progress = document.querySelector(".progress");
const slideCount = document.querySelector(".slide-count");
const previousButton = document.querySelector('[data-action="previous"]');
const nextButton = document.querySelector('[data-action="next"]');
const helpButtons = [...document.querySelectorAll('[data-action="help"]')];
const helpDialog = document.querySelector(".help-dialog");

let activeIndex = 0;
let touchStartX = null;

const clamp = (value, minimum, maximum) =>
  Math.min(Math.max(value, minimum), maximum);

const slideNumber = (index) => String(index + 1).padStart(2, "0");

const indexFromHash = () => {
  const parsed = Number.parseInt(window.location.hash.replace("#", ""), 10);
  return Number.isNaN(parsed) ? 0 : clamp(parsed - 1, 0, slides.length - 1);
};

const updateControls = () => {
  slideCount.textContent = `${slideNumber(activeIndex)} / ${slideNumber(slides.length - 1)}`;
  previousButton.disabled = activeIndex === 0;
  nextButton.disabled = activeIndex === slides.length - 1;

  [...progress.children].forEach((dot, index) => {
    dot.classList.toggle("is-active", index === activeIndex);
    dot.setAttribute("aria-current", index === activeIndex ? "step" : "false");
  });
};

const showSlide = (nextIndex, { updateHash = true } = {}) => {
  const safeIndex = clamp(nextIndex, 0, slides.length - 1);
  if (safeIndex === activeIndex && slides[safeIndex].classList.contains("is-active")) {
    updateControls();
    return;
  }

  const previousSlide = slides[activeIndex];
  const nextSlide = slides[safeIndex];

  previousSlide.classList.remove("is-active");
  previousSlide.classList.add("is-leaving");
  window.setTimeout(() => previousSlide.classList.remove("is-leaving"), 520);

  nextSlide.classList.add("is-active");
  nextSlide.scrollTop = 0;
  activeIndex = safeIndex;

  document.title = `${nextSlide.dataset.title} — UMP Dash`;
  if (updateHash) {
    history.replaceState(null, "", `#${activeIndex + 1}`);
  }
  updateControls();
};

const move = (direction) => showSlide(activeIndex + direction);

const toggleHelp = () => {
  if (helpDialog.open) {
    helpDialog.close();
  } else {
    helpDialog.showModal();
  }
};

const toggleFullscreen = async () => {
  if (document.fullscreenElement) {
    await document.exitFullscreen();
  } else {
    await document.documentElement.requestFullscreen();
  }
};

slides.forEach((slide, index) => {
  slide.classList.toggle("is-active", index === 0);

  const dot = document.createElement("button");
  dot.type = "button";
  dot.setAttribute("aria-label", `Go to slide ${index + 1}: ${slide.dataset.title}`);
  dot.addEventListener("click", () => showSlide(index));
  progress.append(dot);
});

previousButton.addEventListener("click", () => move(-1));
nextButton.addEventListener("click", () => move(1));
helpButtons.forEach((button) => button.addEventListener("click", toggleHelp));

document.addEventListener("keydown", (event) => {
  if (helpDialog.open && event.key !== "Escape" && event.key !== "?") {
    return;
  }

  switch (event.key) {
    case "ArrowRight":
    case "PageDown":
    case " ":
      event.preventDefault();
      move(1);
      break;
    case "ArrowLeft":
    case "PageUp":
      event.preventDefault();
      move(-1);
      break;
    case "Home":
      event.preventDefault();
      showSlide(0);
      break;
    case "End":
      event.preventDefault();
      showSlide(slides.length - 1);
      break;
    case "f":
    case "F":
      event.preventDefault();
      toggleFullscreen();
      break;
    case "?":
      event.preventDefault();
      toggleHelp();
      break;
    default:
      break;
  }
});

document.addEventListener(
  "touchstart",
  (event) => {
    touchStartX = event.changedTouches[0]?.clientX ?? null;
  },
  { passive: true },
);

document.addEventListener(
  "touchend",
  (event) => {
    if (touchStartX === null) return;
    const distance = (event.changedTouches[0]?.clientX ?? touchStartX) - touchStartX;
    if (Math.abs(distance) > 50) move(distance < 0 ? 1 : -1);
    touchStartX = null;
  },
  { passive: true },
);

window.addEventListener("hashchange", () => showSlide(indexFromHash(), { updateHash: false }));

activeIndex = indexFromHash();
slides.forEach((slide, index) => slide.classList.toggle("is-active", index === activeIndex));
document.title = `${slides[activeIndex].dataset.title} — UMP Dash`;
updateControls();
