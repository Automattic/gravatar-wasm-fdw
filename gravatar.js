import crypto from 'crypto';

function getGravatarHash(email) {

  email = email.trim().toLowerCase();

  const hash = crypto
    .createHash('sha256')
    .update(email, 'utf8')
    .digest('hex');

  return hash;
}

const email = 'user@example.com';

const hash = getGravatarHash(email);

const avatarUrl = `https://www.gravatar.com/avatar/${hash}?s=200&d=identicon`;

const profileUrl = `https://www.gravatar.com/${hash}.json`;

console.log("Hash:", hash);
console.log("Avatar:", avatarUrl);
console.log("Profile:", profileUrl);