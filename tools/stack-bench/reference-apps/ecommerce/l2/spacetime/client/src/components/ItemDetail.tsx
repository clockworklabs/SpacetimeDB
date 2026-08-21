import { useState } from 'react';
import { ItemRow, ReviewRow, formatMoney } from '../types';

interface ItemDetailProps {
  item: ItemRow;
  stock: number;
  reviews: ReviewRow[];
  isSignedIn: boolean;
  hasPurchased: boolean;
  onClose: () => void;
  onSubmitReview: (itemId: bigint, rating: number, comment: string) => Promise<void>;
}

export default function ItemDetail({
  item,
  stock,
  reviews,
  isSignedIn,
  hasPurchased,
  onClose,
  onSubmitReview,
}: ItemDetailProps) {
  const [rating, setRating] = useState('5');
  const [comment, setComment] = useState('');
  const [reviewError, setReviewError] = useState<string | null>(null);

  const outOfStock = stock <= 0;
  const avgRating =
    reviews.length === 0
      ? null
      : reviews.reduce((sum, r) => sum + r.rating, 0) / reviews.length;

  const submitReview = async () => {
    setReviewError(null);
    try {
      await onSubmitReview(item.id, Number(rating), comment.trim());
      setComment('');
    } catch (err) {
      setReviewError(err instanceof Error ? err.message : 'Could not submit review.');
    }
  };

  return (
    <div
      className="inline-detail"
      data-testid="item-detail"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      <div className="panel-header">
        <h2>{item.name}</h2>
        <button type="button" className="close-btn" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="panel-body">
        <div className="detail-header">
            <div>
              <div className="muted">SKU {String(item.id)}</div>
              <div>
                Stock: <strong>{stock}</strong>
                {outOfStock && <span className="badge badge-danger" style={{ marginLeft: 8 }}>Out of stock</span>}
              </div>
              <div className="muted">
                Average rating: <span data-testid="review-average">{avgRating === null ? '—' : avgRating.toFixed(1)}</span>{' '}
                ({reviews.length} review{reviews.length === 1 ? '' : 's'})
              </div>
            </div>
            <div className="detail-price">{formatMoney(item.price)}</div>
          </div>

          <div className="review-list">
            <h3 className="section-title">Reviews</h3>
            {reviews.length === 0 && <div className="empty-state">No reviews yet.</div>}
            {reviews.map((r) => (
              <div className="review-item" data-testid="review-item" key={String(r.id)}>
                <div className="review-item-rating">{'★'.repeat(r.rating)}{'☆'.repeat(5 - r.rating)}</div>
                <div>{r.comment}</div>
              </div>
            ))}
          </div>

          {isSignedIn && (
            <div className="review-form">
              <h3 className="section-title">
                {hasPurchased ? 'Write a review' : 'Purchase this item to write a review'}
              </h3>
              <label>
                Rating
                <select
                  data-testid="review-rating"
                  value={rating}
                  onChange={(e) => setRating(e.target.value)}
                  style={{ marginLeft: 8 }}
                >
                  {[5, 4, 3, 2, 1].map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
              <input
                type="text"
                data-testid="review-input"
                placeholder="Share your thoughts..."
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') submitReview();
                }}
              />
              <button type="button" className="btn btn-primary" data-testid="review-submit" onClick={submitReview}>
                Submit review
              </button>
              {reviewError && (
                <div className="error-text" data-testid="review-error">
                  {reviewError}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
  );
}
