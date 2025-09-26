import "./FlowLogo.css";

function FlowLogo() {
  return (
    <div className="flow-logo">
      <svg
        width="120"
        height="120"
        viewBox="0 0 120 120"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="flow-logo-svg"
      >
        <circle cx="60" cy="60" r="50" fill="url(#gradient)" />
        <path
          d="M30 45 Q60 25 90 45 Q60 65 30 45"
          fill="white"
          fillOpacity="0.9"
        />
        <path
          d="M30 75 Q60 55 90 75 Q60 95 30 75"
          fill="white"
          fillOpacity="0.7"
        />
        <defs>
          <linearGradient id="gradient" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#667eea" />
            <stop offset="100%" stopColor="#764ba2" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}

export default FlowLogo;
