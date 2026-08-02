-- This migration only changes seeded display labels. Do not remove user data.

UPDATE budget_groups SET name = CASE key
  WHEN 'needs' THEN 'Needs' WHEN 'wants' THEN 'Wants' WHEN 'savings' THEN 'Savings'
  WHEN 'giving' THEN 'Giving' WHEN 'personal' THEN 'Personal' WHEN 'other' THEN 'Other'
  ELSE name END
WHERE is_system = 1;

UPDATE taxonomies SET name = CASE id
  WHEN 'spending_categories' THEN 'Spending Categories'
  WHEN 'income_sources' THEN 'Income Sources'
  WHEN 'savings_categories' THEN 'Savings'
  ELSE name END
WHERE id IN ('spending_categories', 'income_sources', 'savings_categories');

UPDATE spending_event_types SET name = CASE key
  WHEN 'travel' THEN 'Travel' WHEN 'holiday' THEN 'Holiday' WHEN 'business' THEN 'Business'
  WHEN 'education' THEN 'Education' WHEN 'medical' THEN 'Medical'
  WHEN 'special_occasion' THEN 'Special Occasion' WHEN 'other' THEN 'Other'
  ELSE name END
WHERE key IN ('travel', 'holiday', 'business', 'education', 'medical', 'special_occasion', 'other');
