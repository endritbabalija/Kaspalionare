import { useEffect, useState } from 'react';
import { routeOf, type Route } from './routes';

export function useRoute(): Route {
  const [r, setR] = useState<Route>(() => routeOf(window.location.hash, import.meta.env.DEV));
  useEffect(() => {
    const on = () => setR(routeOf(window.location.hash, import.meta.env.DEV));
    window.addEventListener('hashchange', on);
    return () => window.removeEventListener('hashchange', on);
  }, []);
  return r;
}
