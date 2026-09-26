export interface User {
  id: string;
  name: string;
  age: number;
  city: string;
}

const names = ['Mara', 'luca', 'Ada', 'Zoe', 'bruno', 'Carla', 'Ivo', 'Elena', 'Dario', 'Fabio', 'Gina', 'Hugo',
  'Irene', 'Jonas', 'Katia', 'Leo', 'Mia', 'Nico', 'Olga', 'Piero', 'Quinn', 'Rita', 'Sara', 'Tom', 'Ugo'];

export const users: User[] = names.map((name, i) => ({
  id: `u${i + 1}`,
  name,
  age: 18 + ((i * 7) % 50),
  city: i % 3 === 0 ? 'Milano' : i % 3 === 1 ? 'Roma' : 'Torino',
}));
