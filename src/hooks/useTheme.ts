import { useState, useEffect } from 'react';

export function useTheme(themeSetting: 'system' | 'light' | 'dark') {
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    if (themeSetting === 'dark') {
      document.documentElement.classList.add('dark');
      setIsDark(true);
    } else if (themeSetting === 'light') {
      document.documentElement.classList.remove('dark');
      setIsDark(false);
    } else {
      // System preference
      const media = window.matchMedia('(prefers-color-scheme: dark)');
      const handler = (e: MediaQueryListEvent) => {
        document.documentElement.classList.toggle('dark', e.matches);
        setIsDark(e.matches);
      };
      document.documentElement.classList.toggle('dark', media.matches);
      setIsDark(media.matches);
      media.addEventListener('change', handler);
      return () => media.removeEventListener('change', handler);
    }
  }, [themeSetting]);

  return { isDark };
}
