import { createContext, useContext, useState, useEffect, ReactNode } from 'react';

interface User {
  did: string;
}

interface AuthContextType {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  login: (token: string, did: string) => void;
  logout: () => void;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const savedToken = localStorage.getItem('token');
    const savedDid = localStorage.getItem('did');

    if (savedToken && savedDid) {
      setToken(savedToken);
      setUser({ did: savedDid });
    }

    setIsLoading(false);
  }, []);

  const login = (newToken: string, did: string) => {
    localStorage.setItem('token', newToken);
    localStorage.setItem('did', did);
    setToken(newToken);
    setUser({ did });
  };

  const logout = () => {
    localStorage.removeItem('token');
    localStorage.removeItem('did');
    setToken(null);
    setUser(null);
  };

  return (
    <AuthContext.Provider
      value={{
        user,
        token,
        isAuthenticated: !!token,
        isLoading,
        login,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
