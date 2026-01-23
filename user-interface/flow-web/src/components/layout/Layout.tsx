import { ReactNode } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../../contexts/AuthContext';
import { useKeyboardShortcut } from '../../hooks/useKeyboardShortcut';
import { NetworkIndicator } from '../network';
import './Layout.css';

interface LayoutProps {
  children: ReactNode;
}

export function Layout({ children }: LayoutProps) {
  const { isAuthenticated, logout, user } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();

  // Global keyboard shortcut: Cmd/Ctrl + K to open search
  useKeyboardShortcut({
    key: 'k',
    withModifier: true,
    onTrigger: () => navigate('/search'),
    enabled: isAuthenticated,
  });

  const isActive = (path: string) => location.pathname === path;

  return (
    <div className="layout">
      <nav className="sidebar">
        <div className="sidebar-header">
          <Link to="/" className="logo">
            <span className="logo-icon">F</span>
            <span className="logo-text">Flow</span>
          </Link>
        </div>

        <div className="sidebar-nav">
          {isAuthenticated ? (
            <>
              <Link to="/" className={`nav-item ${isActive('/') ? 'active' : ''}`}>
                <span className="nav-icon">🏠</span>
                <span>Home</span>
              </Link>
              <Link to="/search" className={`nav-item ${isActive('/search') ? 'active' : ''}`}>
                <span className="nav-icon">🔍</span>
                <span>Search</span>
              </Link>
              <Link to="/settings" className={`nav-item ${isActive('/settings') ? 'active' : ''}`}>
                <span className="nav-icon">⚙️</span>
                <span>Settings</span>
              </Link>
            </>
          ) : (
            <Link to="/auth" className={`nav-item ${isActive('/auth') ? 'active' : ''}`}>
              <span className="nav-icon">🔑</span>
              <span>Login</span>
            </Link>
          )}
        </div>

        {isAuthenticated && (
          <div className="sidebar-footer">
            <NetworkIndicator showPeerCount />
            <div className="user-info">
              <span className="user-did" title={user?.did}>
                {user?.did?.slice(0, 20)}...
              </span>
            </div>
            <button onClick={logout} className="logout-btn">
              Logout
            </button>
          </div>
        )}
      </nav>

      <main className="main-content">{children}</main>
    </div>
  );
}
